// Copyright 2017 TiKV Project Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package server

import (
	"context"
	"fmt"
	"strings"

	"github.com/pkg/errors"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/kvpb"
	"github.com/AutoMQ/pd/pkg/util/etcdutil"
)

var _globalPrefix = []byte("/global/kv/")

// GrpcServer wraps Server to provide grpc service.
type GrpcServer struct {
	*Server
	lg *zap.Logger
}

func NewGrpcServer(s *Server, lg *zap.Logger) *GrpcServer {
	return &GrpcServer{
		Server: s,
		lg:     lg.With(zap.String("grpc-server", "kv")),
	}
}

// Store stores kv into etcd by transaction
func (s *GrpcServer) Store(ctx context.Context, request *kvpb.StoreRequest) (*kvpb.StoreResponse, error) {
	logger := s.lg
	prefix := request.GetPrefix()
	if prefix == nil {
		prefix = _globalPrefix
	}
	builder := strings.Builder{}
	ops := make([]clientv3.Op, len(request.Changes))
	for i, item := range request.Changes {
		builder.Reset()
		builder.Write(prefix)
		builder.Write(item.GetName())
		switch item.GetKind() {
		case kvpb.EventType_PUT:
			value := string(item.GetPayload())
			ops[i] = clientv3.OpPut(builder.String(), value)
		case kvpb.EventType_DELETE:
			ops[i] = clientv3.OpDelete(builder.String())
		}
	}
	res, err :=
		etcdutil.NewTxn(ctx, s.client, logger).Then(ops...).Commit()
	if err != nil {
		return &kvpb.StoreResponse{}, err
	}
	if !res.Succeeded {
		return &kvpb.StoreResponse{}, errors.New("failed to execute Store transaction")
	}
	return &kvpb.StoreResponse{}, nil
}

// Load support 2 ways to load kv from etcd
// - `names` iteratively get value from "${prefix}${name}" but not care about revision
// - `prefix` if `names` is nil can get all values and revision of the path
func (s *GrpcServer) Load(ctx context.Context, request *kvpb.LoadRequest) (*kvpb.LoadResponse, error) {
	prefix := request.GetPrefix()
	if prefix == nil {
		prefix = _globalPrefix
	}
	if request.Names != nil {
		builder := strings.Builder{}
		res := make([]*kvpb.Item, len(request.Names))
		for i, name := range request.Names {
			builder.Reset()
			builder.Write(prefix)
			builder.Write(name)
			r, err := s.client.Get(ctx, builder.String())
			switch {
			case err != nil:
				res[i] = &kvpb.Item{Name: name, Error: &kvpb.Error{Type: kvpb.ErrorType_UNKNOWN, Message: err.Error()}}
			case len(r.Kvs) == 0:
				res[i] = &kvpb.Item{Name: name, Error: &kvpb.Error{Type: kvpb.ErrorType_NOT_FOUND, Message: fmt.Sprintf("key %s not found", name)}}
			default:
				res[i] = &kvpb.Item{Name: name, Payload: r.Kvs[0].Value, Kind: kvpb.EventType_PUT}
			}
		}
		return &kvpb.LoadResponse{Items: res}, nil
	}
	r, err := s.client.Get(ctx, string(prefix), clientv3.WithPrefix())
	if err != nil {
		return &kvpb.LoadResponse{}, err
	}
	res := make([]*kvpb.Item, len(r.Kvs))
	for i, value := range r.Kvs {
		res[i] = &kvpb.Item{Kind: kvpb.EventType_PUT, Name: value.Key, Payload: value.Value}
	}
	return &kvpb.LoadResponse{Items: res, Revision: r.Header.GetRevision()}, nil
}

// Watch on revision which greater than or equal to the required revision.
// if the connection of Watch is end or stopped by whatever reason, just reconnect to it.
func (s *GrpcServer) Watch(req *kvpb.WatchRequest, server kvpb.KV_WatchServer) error {
	ctx, cancel := context.WithCancel(s.Context())
	defer cancel()
	prefix := req.GetPrefix()
	if prefix == nil {
		prefix = _globalPrefix
	}
	revision := req.GetRevision()
	// If the revision is compacted, will meet required revision has been compacted error.
	// - If required revision < CompactRevision, we need to reload all configs to avoid losing data.
	// - If required revision >= CompactRevision, just keep watching.
	watchChan := s.client.Watch(ctx, string(prefix), clientv3.WithPrefix(), clientv3.WithRev(revision))
	for {
		select {
		case <-ctx.Done():
			return nil
		case res := <-watchChan:
			if revision < res.CompactRevision {
				if err := server.Send(&kvpb.WatchResponse{
					Revision: res.CompactRevision,
					Error: &kvpb.Error{
						Type:    kvpb.ErrorType_DATA_COMPACTED,
						Message: fmt.Sprintf("required watch revision: %d is smaller than current compact/min revision. %d", revision, res.CompactRevision),
					},
				}); err != nil {
					return err
				}
			}
			revision = res.Header.GetRevision()

			cfgs := make([]*kvpb.Item, 0, len(res.Events))
			for _, e := range res.Events {
				cfgs = append(cfgs, &kvpb.Item{Name: e.Kv.Key, Payload: e.Kv.Value, Kind: kvpb.EventType(e.Type)})
			}
			if len(cfgs) > 0 {
				if err := server.Send(&kvpb.WatchResponse{Changes: cfgs, Revision: res.Header.GetRevision()}); err != nil {
					return err
				}
			}
		}
	}
}
