package sdk.elastic.storage.client.common;

import sdk.elastic.storage.client.route.Address;
import sdk.elastic.storage.flatc.header.PlacementManagerCluster;
import sdk.elastic.storage.flatc.header.Status;

import static sdk.elastic.storage.flatc.header.ErrorCode.PM_NOT_LEADER;

public class PmUtil {
    /**
     * Extract the new leader Pm address from the status detail.
     * If the status code is not {@link sdk.elastic.storage.flatc.header.ErrorCode#PM_NOT_LEADER}, return null.
     *
     * @param status the status in the response header
     * @return the new leader address
     */
    public static Address extractNewPmAddress(Status status) {
        if (status.code() != PM_NOT_LEADER) {
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
