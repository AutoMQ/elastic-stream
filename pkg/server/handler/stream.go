package handler

import (
	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func (s *Sbp) CreateStreams(req *protocol.CreateStreamsRequest) (resp *protocol.CreateStreamsResponse) {
	resp = &protocol.CreateStreamsResponse{}
	if !s.c.IsLeader() {
		s.notLeaderError(resp)
		return
	}

	streams, err := s.c.CreateStreams(req.Streams)
	if err != nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeUNKNOWN, Message: err.Error()})
		return
	}

	resp.CreateResponses = make([]*rpcfb.CreateStreamResultT, 0, len(streams))
	for _, stream := range streams {
		resp.CreateResponses = append(resp.CreateResponses, &rpcfb.CreateStreamResultT{Stream: stream})
	}
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
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeUNKNOWN, Message: err.Error()})
		return
	}

	resp.DeleteResponses = make([]*rpcfb.DeleteStreamResultT, 0, len(streams))
	for _, stream := range streams {
		resp.DeleteResponses = append(resp.DeleteResponses, &rpcfb.DeleteStreamResultT{DeletedStream: stream})
	}
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
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeUNKNOWN, Message: err.Error()})
		return
	}

	resp.UpdateResponses = make([]*rpcfb.UpdateStreamResultT, 0, len(streams))
	for _, stream := range streams {
		resp.UpdateResponses = append(resp.UpdateResponses, &rpcfb.UpdateStreamResultT{Stream: stream})
	}
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
	return
}
