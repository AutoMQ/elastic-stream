package handler

import (
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	sbpServer "github.com/AutoMQ/placement-manager/pkg/sbp/server"
)

type CheckAble interface {
	sbpServer.Handler

	// Check checks the request and response, and returns true if the check passes.
	Check(req protocol.InRequest, resp protocol.OutResponse) (pass bool)
}

type Checker struct {
	Handler CheckAble
}

func (c Checker) Heartbeat(req *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse) {
	// all pm nodes will handle heartbeat request, so we don't need to check leader here.
	c.Handler.Heartbeat(req, resp)
}

func (c Checker) AllocateID(req *protocol.IDAllocationRequest, resp *protocol.IDAllocationResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.AllocateID(req, resp)
}

func (c Checker) ListRanges(req *protocol.ListRangesRequest, resp *protocol.ListRangesResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.ListRanges(req, resp)
}

func (c Checker) SealRanges(req *protocol.SealRangesRequest, resp *protocol.SealRangesResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.SealRanges(req, resp)
}

func (c Checker) CreateStreams(req *protocol.CreateStreamsRequest, resp *protocol.CreateStreamsResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.CreateStreams(req, resp)
}

func (c Checker) DeleteStreams(req *protocol.DeleteStreamsRequest, resp *protocol.DeleteStreamsResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.DeleteStreams(req, resp)
}

func (c Checker) UpdateStreams(req *protocol.UpdateStreamsRequest, resp *protocol.UpdateStreamsResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.UpdateStreams(req, resp)
}

func (c Checker) DescribeStreams(req *protocol.DescribeStreamsRequest, resp *protocol.DescribeStreamsResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.DescribeStreams(req, resp)
}

func (c Checker) DescribePMCluster(req *protocol.DescribePMClusterRequest, resp *protocol.DescribePMClusterResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.DescribePMCluster(req, resp)
}
