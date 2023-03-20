package handler

import (
	"github.com/pkg/errors"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
)

func (s *Sbp) CreateStreams(req *protocol.CreateStreamsRequest) (resp *protocol.CreateStreamsResponse) {
	resp = &protocol.CreateStreamsResponse{}
	if !s.c.IsLeader() {
		s.notLeaderError(resp)
		return
	}

	results, err := s.c.CreateStreams(req.Streams)
	if err != nil {
		if errors.Is(err, cluster.ErrNotEnoughDataNodes) {
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NO_AVAILABLE_DN, Message: err.Error()})
			return
		}
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		return
	}

	for _, result := range results {
		result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
	}
	resp.CreateResponses = results
	resp.OK()
	return
}

func (s *Sbp) DeleteStreams(req *protocol.DeleteStreamsRequest) (resp *protocol.DeleteStreamsResponse) {
	resp = &protocol.DeleteStreamsResponse{}
	if !s.c.IsLeader() {
		s.notLeaderError(resp)
		return
	}

	streamIDs := make([]int64, 0, len(req.Streams))
	for _, stream := range req.Streams {
		streamIDs = append(streamIDs, stream.StreamId)
	}
	streams, err := s.c.DeleteStreams(streamIDs)
	if err != nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		return
	}

	resp.DeleteResponses = make([]*rpcfb.DeleteStreamResultT, 0, len(streams))
	for _, stream := range streams {
		resp.DeleteResponses = append(resp.DeleteResponses, &rpcfb.DeleteStreamResultT{
			DeletedStream: stream,
			Status:        &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK},
		})
	}
	resp.OK()
	return
}

func (s *Sbp) UpdateStreams(req *protocol.UpdateStreamsRequest) (resp *protocol.UpdateStreamsResponse) {
	resp = &protocol.UpdateStreamsResponse{}
	if !s.c.IsLeader() {
		s.notLeaderError(resp)
		return
	}

	streams, err := s.c.UpdateStreams(req.Streams)
	if err != nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		return
	}

	resp.UpdateResponses = make([]*rpcfb.UpdateStreamResultT, 0, len(streams))
	for _, stream := range streams {
		resp.UpdateResponses = append(resp.UpdateResponses, &rpcfb.UpdateStreamResultT{
			Stream: stream,
			Status: &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK},
		})
	}
	resp.OK()
	return
}

func (s *Sbp) DescribeStreams(req *protocol.DescribeStreamsRequest) (resp *protocol.DescribeStreamsResponse) {
	resp = &protocol.DescribeStreamsResponse{}
	if !s.c.IsLeader() {
		s.notLeaderError(resp)
		return
	}

	result := s.c.DescribeStreams(req.StreamIds)
	resp.DescribeResponses = result
	resp.OK()
	return
}
