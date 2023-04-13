package sdk.elastic.stream.models;

import sdk.elastic.stream.flatc.header.DataNodeT;
import sdk.elastic.stream.flatc.header.RangeT;

public class RangeTDecorator extends RangeT {
    protected RangeT decoratedRangeT;

    public RangeTDecorator(RangeT rangeT) {
        this.decoratedRangeT = rangeT;
    }

    /**
     * Get primary data node.
     *
     * @return primary data node
     */
    public DataNodeT getPrimaryNode() {
        return decoratedRangeT.getReplicaNodes()[getPrimaryDnIndex()].getDataNode();
    }

    /**
     * Get the index of the next data node.
     *
     * @param index current data node index
     * @return index of the next data node
     */
    public int getNextDnIndex(int index) {
        if (index >= decoratedRangeT.getReplicaNodes().length - 1 || index < 0) {
            return 0;
        }
        return index + 1;
    }

    /**
     * Get the index of the primary data node.
     *
     * @return index of the primary data node
     */
    public int getPrimaryDnIndex() {
        for (int i = 0; i < decoratedRangeT.getReplicaNodes().length; i++) {
            if (decoratedRangeT.getReplicaNodes()[i].getIsPrimary()) {
                return i;
            }
        }
        return -1;
    }
}
