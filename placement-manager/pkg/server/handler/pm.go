package handler

import (
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func (h *Handler) DescribePMCluster(_ *protocol.DescribePMClusterRequest, resp *protocol.DescribePMClusterResponse) {
	resp.Cluster = h.pmCluster()
	resp.OK()
}
