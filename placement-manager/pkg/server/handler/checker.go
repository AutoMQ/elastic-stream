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

func (c Checker) ListRange(req *protocol.ListRangeRequest, resp *protocol.ListRangeResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.ListRange(req, resp)
}

func (c Checker) SealRange(req *protocol.SealRangeRequest, resp *protocol.SealRangeResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.SealRange(req, resp)
}

func (c Checker) CreateRange(req *protocol.CreateRangeRequest, resp *protocol.CreateRangeResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.CreateRange(req, resp)
}

func (c Checker) CreateStream(req *protocol.CreateStreamRequest, resp *protocol.CreateStreamResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.CreateStream(req, resp)
}

func (c Checker) DeleteStream(req *protocol.DeleteStreamRequest, resp *protocol.DeleteStreamResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.DeleteStream(req, resp)
}

func (c Checker) UpdateStream(req *protocol.UpdateStreamRequest, resp *protocol.UpdateStreamResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.UpdateStream(req, resp)
}

func (c Checker) DescribeStream(req *protocol.DescribeStreamRequest, resp *protocol.DescribeStreamResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.DescribeStream(req, resp)
}

func (c Checker) ReportMetrics(req *protocol.ReportMetricsRequest, resp *protocol.ReportMetricsResponse) {
	// all pm nodes will handle report metrics request, so we don't need to check leader here.
	c.Handler.ReportMetrics(req, resp)
}

func (c Checker) DescribePMCluster(req *protocol.DescribePMClusterRequest, resp *protocol.DescribePMClusterResponse) {
	if !c.Handler.Check(req, resp) {
		return
	}
	c.Handler.DescribePMCluster(req, resp)
}
