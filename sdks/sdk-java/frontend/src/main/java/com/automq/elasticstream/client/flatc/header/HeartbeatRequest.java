// automatically generated by the FlatBuffers compiler, do not modify

package com.automq.elasticstream.client.flatc.header;

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
public final class HeartbeatRequest extends Table {
  public static void ValidateVersion() { Constants.FLATBUFFERS_23_3_3(); }
  public static HeartbeatRequest getRootAsHeartbeatRequest(ByteBuffer _bb) { return getRootAsHeartbeatRequest(_bb, new HeartbeatRequest()); }
  public static HeartbeatRequest getRootAsHeartbeatRequest(ByteBuffer _bb, HeartbeatRequest obj) { _bb.order(ByteOrder.LITTLE_ENDIAN); return (obj.__assign(_bb.getInt(_bb.position()) + _bb.position(), _bb)); }
  public void __init(int _i, ByteBuffer _bb) { __reset(_i, _bb); }
  public HeartbeatRequest __assign(int _i, ByteBuffer _bb) { __init(_i, _bb); return this; }

  public String clientId() { int o = __offset(4); return o != 0 ? __string(o + bb_pos) : null; }
  public ByteBuffer clientIdAsByteBuffer() { return __vector_as_bytebuffer(4, 1); }
  public ByteBuffer clientIdInByteBuffer(ByteBuffer _bb) { return __vector_in_bytebuffer(_bb, 4, 1); }
  public byte clientRole() { int o = __offset(6); return o != 0 ? bb.get(o + bb_pos) : 0; }
  public com.automq.elasticstream.client.flatc.header.DataNode dataNode() { return dataNode(new com.automq.elasticstream.client.flatc.header.DataNode()); }
  public com.automq.elasticstream.client.flatc.header.DataNode dataNode(com.automq.elasticstream.client.flatc.header.DataNode obj) { int o = __offset(8); return o != 0 ? obj.__assign(__indirect(o + bb_pos), bb) : null; }

  public static int createHeartbeatRequest(FlatBufferBuilder builder,
      int clientIdOffset,
      byte clientRole,
      int dataNodeOffset) {
    builder.startTable(3);
    HeartbeatRequest.addDataNode(builder, dataNodeOffset);
    HeartbeatRequest.addClientId(builder, clientIdOffset);
    HeartbeatRequest.addClientRole(builder, clientRole);
    return HeartbeatRequest.endHeartbeatRequest(builder);
  }

  public static void startHeartbeatRequest(FlatBufferBuilder builder) { builder.startTable(3); }
  public static void addClientId(FlatBufferBuilder builder, int clientIdOffset) { builder.addOffset(0, clientIdOffset, 0); }
  public static void addClientRole(FlatBufferBuilder builder, byte clientRole) { builder.addByte(1, clientRole, 0); }
  public static void addDataNode(FlatBufferBuilder builder, int dataNodeOffset) { builder.addOffset(2, dataNodeOffset, 0); }
  public static int endHeartbeatRequest(FlatBufferBuilder builder) {
    int o = builder.endTable();
    return o;
  }

  public static final class Vector extends BaseVector {
    public Vector __assign(int _vector, int _element_size, ByteBuffer _bb) { __reset(_vector, _element_size, _bb); return this; }

    public HeartbeatRequest get(int j) { return get(new HeartbeatRequest(), j); }
    public HeartbeatRequest get(HeartbeatRequest obj, int j) {  return obj.__assign(__indirect(__element(j), bb), bb); }
  }
  public HeartbeatRequestT unpack() {
    HeartbeatRequestT _o = new HeartbeatRequestT();
    unpackTo(_o);
    return _o;
  }
  public void unpackTo(HeartbeatRequestT _o) {
    String _oClientId = clientId();
    _o.setClientId(_oClientId);
    byte _oClientRole = clientRole();
    _o.setClientRole(_oClientRole);
    if (dataNode() != null) _o.setDataNode(dataNode().unpack());
    else _o.setDataNode(null);
  }
  public static int pack(FlatBufferBuilder builder, HeartbeatRequestT _o) {
    if (_o == null) return 0;
    int _clientId = _o.getClientId() == null ? 0 : builder.createString(_o.getClientId());
    int _dataNode = _o.getDataNode() == null ? 0 : com.automq.elasticstream.client.flatc.header.DataNode.pack(builder, _o.getDataNode());
    return createHeartbeatRequest(
      builder,
      _clientId,
      _o.getClientRole(),
      _dataNode);
  }
}

