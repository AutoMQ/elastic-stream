// automatically generated by the FlatBuffers compiler, do not modify

package header;

import com.google.flatbuffers.BaseVector;
import com.google.flatbuffers.BooleanVector;
import com.google.flatbuffers.ByteVector;
import com.google.flatbuffers.Constants;
import com.google.flatbuffers.DoubleVector;
import com.google.flatbuffers.FlatBufferBuilder;
import com.google.flatbuffers.FloatVector;
import com.google.flatbuffers.IntVector;
import com.google.flatbuffers.LongVector;
import com.google.flatbuffers.ShortVector;
import com.google.flatbuffers.StringVector;
import com.google.flatbuffers.Struct;
import com.google.flatbuffers.Table;
import com.google.flatbuffers.UnionVector;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

@SuppressWarnings("unused")
public final class ReportMetricsResponse extends Table {
  public static void ValidateVersion() { Constants.FLATBUFFERS_23_1_21(); }
  public static ReportMetricsResponse getRootAsReportMetricsResponse(ByteBuffer _bb) { return getRootAsReportMetricsResponse(_bb, new ReportMetricsResponse()); }
  public static ReportMetricsResponse getRootAsReportMetricsResponse(ByteBuffer _bb, ReportMetricsResponse obj) { _bb.order(ByteOrder.LITTLE_ENDIAN); return (obj.__assign(_bb.getInt(_bb.position()) + _bb.position(), _bb)); }
  public void __init(int _i, ByteBuffer _bb) { __reset(_i, _bb); }
  public ReportMetricsResponse __assign(int _i, ByteBuffer _bb) { __init(_i, _bb); return this; }

  public header.DataNode dataNode() { return dataNode(new header.DataNode()); }
  public header.DataNode dataNode(header.DataNode obj) { int o = __offset(4); return o != 0 ? obj.__assign(__indirect(o + bb_pos), bb) : null; }
  public header.Status status() { return status(new header.Status()); }
  public header.Status status(header.Status obj) { int o = __offset(6); return o != 0 ? obj.__assign(__indirect(o + bb_pos), bb) : null; }

  public static int createReportMetricsResponse(FlatBufferBuilder builder,
      int dataNodeOffset,
      int statusOffset) {
    builder.startTable(2);
    ReportMetricsResponse.addStatus(builder, statusOffset);
    ReportMetricsResponse.addDataNode(builder, dataNodeOffset);
    return ReportMetricsResponse.endReportMetricsResponse(builder);
  }

  public static void startReportMetricsResponse(FlatBufferBuilder builder) { builder.startTable(2); }
  public static void addDataNode(FlatBufferBuilder builder, int dataNodeOffset) { builder.addOffset(0, dataNodeOffset, 0); }
  public static void addStatus(FlatBufferBuilder builder, int statusOffset) { builder.addOffset(1, statusOffset, 0); }
  public static int endReportMetricsResponse(FlatBufferBuilder builder) {
    int o = builder.endTable();
    return o;
  }

  public static final class Vector extends BaseVector {
    public Vector __assign(int _vector, int _element_size, ByteBuffer _bb) { __reset(_vector, _element_size, _bb); return this; }

    public ReportMetricsResponse get(int j) { return get(new ReportMetricsResponse(), j); }
    public ReportMetricsResponse get(ReportMetricsResponse obj, int j) {  return obj.__assign(__indirect(__element(j), bb), bb); }
  }
  public ReportMetricsResponseT unpack() {
    ReportMetricsResponseT _o = new ReportMetricsResponseT();
    unpackTo(_o);
    return _o;
  }
  public void unpackTo(ReportMetricsResponseT _o) {
    if (dataNode() != null) _o.setDataNode(dataNode().unpack());
    else _o.setDataNode(null);
    if (status() != null) _o.setStatus(status().unpack());
    else _o.setStatus(null);
  }
  public static int pack(FlatBufferBuilder builder, ReportMetricsResponseT _o) {
    if (_o == null) return 0;
    int _dataNode = _o.getDataNode() == null ? 0 : header.DataNode.pack(builder, _o.getDataNode());
    int _status = _o.getStatus() == null ? 0 : header.Status.pack(builder, _o.getStatus());
    return createReportMetricsResponse(
      builder,
      _dataNode,
      _status);
  }
}

