package client.common;

import client.route.Address;
import header.PlacementManager;
import header.Status;

import static header.ErrorCode.PM_NOT_LEADER;

public class PmUtil {
    /**
     * Extract the new leader Pm address from the status detail.
     * If the status code is not {@link header.ErrorCode#PM_NOT_LEADER}, return null.
     *
     * @param status the status in the response header
     * @return the new leader address
     */
    public static Address extractNewPmAddress(Status status) {
        if (status.code() != PM_NOT_LEADER) {
            return null;
        }

        PlacementManager manager = PlacementManager.getRootAsPlacementManager(FlatBuffersUtil.byteVector2ByteBuffer(status.detailVector()));
        for (int i = 0; i < manager.nodesLength(); i++) {
            if (manager.nodes(i).isLeader()) {
                String hostPortString = manager.nodes(i).advertiseAddr();
                return Address.fromAddress(hostPortString);
            }
        }
        return null;
    }
}
