package sdk.elastic.stream.client.common;

import sdk.elastic.stream.flatc.header.DataNodeT;
import sdk.elastic.stream.flatc.header.RangeT;

public class DnUtil {
    /**
     * Find primary data node.
     *
     * @param rangeT range
     * @return primary data node
     */
    public static DataNodeT findPrimaryDn(RangeT rangeT) {
        return rangeT.getReplicaNodes()[findPrimaryDnIndex(rangeT)].getDataNode();
    }

    public static int findNextDnIndex(RangeT rangeT, int index) {
        if (index >= rangeT.getReplicaNodes().length - 1) {
            return 0;
        }
        return index + 1;
    }

    public static int findPrimaryDnIndex(RangeT rangeT) {
        for (int i = 0; i < rangeT.getReplicaNodes().length; i++) {
            if (rangeT.getReplicaNodes()[i].getIsPrimary()) {
                return i;
            }
        }
        return -1;
    }
}
