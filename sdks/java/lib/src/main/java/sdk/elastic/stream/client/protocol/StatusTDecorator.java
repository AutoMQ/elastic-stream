package sdk.elastic.stream.client.protocol;

import sdk.elastic.stream.client.common.FlatBuffersUtil;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.PlacementManagerCluster;
import sdk.elastic.stream.flatc.header.Status;

import static sdk.elastic.stream.flatc.header.ErrorCode.PM_NOT_LEADER;

public class StatusTDecorator {
    protected Status status;

    public StatusTDecorator(Status status) {
        this.status = status;
    }

    public Status getStatus() {
        return status;
    }

    /**
     * Check if the status refers to a leader PM address change.
     *
     * @return true if the status refers to a leader PM address change.
     */
    public boolean isLeaderPmAddressChanged() {
        return status.code() == PM_NOT_LEADER;
    }

    /**
     * Extract the new leader PM address from the status.
     * If the status does not refer to a leader PM address change, return null.
     *
     * @return the new leader PM address
     */
    public Address maybeGetNewPmAddress() {
        if (!isLeaderPmAddressChanged()) {
            return null;
        }

        PlacementManagerCluster manager = PlacementManagerCluster.getRootAsPlacementManagerCluster(FlatBuffersUtil.byteVector2ByteBuffer(status.detailVector()));
        for (int i = 0; i < manager.nodesLength(); i++) {
            if (manager.nodes(i).isLeader()) {
                String hostPortString = manager.nodes(i).advertiseAddr();
                return Address.fromAddress(hostPortString);
            }
        }
        return null;
    }

}
