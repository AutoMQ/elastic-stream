package sdk.elastic.stream.client.protocol;

import io.netty.buffer.ByteBuf;
import io.netty.util.ReferenceCounted;
import java.nio.ByteBuffer;
import sdk.elastic.stream.client.common.Utilities;

public class SbpFrame implements RemotingItem {
    public static byte DEFAULT_HEADER_FORMAT = 0x01;
    public static final byte DEFAULT_MAGIC_CODE = 23;
    public static final int MAX_HEADER_LENGTH = 0xFFFFFF;
    public static final int FRAME_SIZE_LENGTH = 4;
    public static final int MIN_FRAME_SIZE = 8 + 4 + 4;
    public static final byte DEFAULT_REQUEST_FLAG = 0x00;
    public static final byte GENERAL_RESPONSE_FLAG = 0x01;
    public static final byte ENDING_RESPONSE_FLAG = 0x02;
    public static final byte ERROR_RESPONSE_FLAG = 0x04;
    private final int length;
    private final byte magicCode;
    private final short operationCode;
    private final byte flag;
    private int streamId;
    private final byte headerFormat;
    private final int headerLength;
    private final ByteBuffer header;
    private final ByteBuffer[] payload;
    private final int payloadCrc;
    private final ByteBuf byteBuf;

    SbpFrame(byte magicCode, short operationCode, byte flag, int streamId, byte headerFormat, ByteBuffer header,
        ByteBuffer[] payload) {
        this(magicCode, operationCode, flag, streamId, headerFormat, header, payload, null);
    }

    SbpFrame(byte magicCode, short operationCode, byte flag, int streamId, byte headerFormat, ByteBuffer header,
        ByteBuffer[] payload, ByteBuf byteBuf) {
        if (header != null && header.remaining() > SbpFrame.MAX_HEADER_LENGTH) {
            throw new IllegalArgumentException("header length should not exceed " + SbpFrame.MAX_HEADER_LENGTH + " bytes");
        }
        this.magicCode = magicCode;
        this.operationCode = operationCode;
        this.flag = flag;
        this.streamId = streamId;
        this.headerFormat = headerFormat;
        this.headerLength = header == null ? 0 : header.remaining();
        this.header = header;
        this.payload = payload;
        this.payloadCrc = (int) Utilities.crc32Calculation(payload);
        this.length = calculateLength();
        this.byteBuf = byteBuf;
    }

    public SbpFrame(ByteBuf buf) {
        assert buf.readableBytes() >= MIN_FRAME_SIZE;

        this.length = buf.readInt();
        if (this.length != buf.readableBytes()) {
            throw new IllegalArgumentException("inconsistent frame length");
        }

        this.magicCode = buf.readByte();
        this.operationCode = buf.readShort();
        this.flag = buf.readByte();
        this.streamId = buf.readInt();
        this.headerFormat = buf.readByte();
        this.headerLength = (buf.readByte() & 0xff) << 16 | (buf.readByte() & 0xff) << 8 | (buf.readByte() & 0xff);
        if (this.headerLength > 0) {
            this.header = buf.nioBuffer(buf.readerIndex(), this.headerLength);
            buf.skipBytes(this.headerLength);
        } else {
            this.header = null;
        }
        if (buf.readableBytes() > 4) {
            int payloadLength = buf.readableBytes() - 4;
            // actually there is only one ByteBuffer in payload
            this.payload = new ByteBuffer[] {buf.nioBuffer(buf.readerIndex(), payloadLength)};
            buf.skipBytes(payloadLength);
        } else {
            this.payload = null;
        }
        this.payloadCrc = (int) Utilities.crc32Calculation(payload);
        int recordedCrc = buf.readInt();

        if (!this.checkPayloadCrc(recordedCrc)) {
            throw new IllegalArgumentException("Frame payload crc check failed");
        }

        this.byteBuf = buf;
    }

    public byte[] encode() {
        ByteBuffer result = ByteBuffer.allocate(length + FRAME_SIZE_LENGTH);

        result.putInt(length);
        result.put(magicCode);
        result.putShort(operationCode);
        result.put(flag);
        result.putInt(streamId);
        result.put(headerFormat);
        result.put(new byte[] {
            (byte) ((headerLength >> 16) & 0xff),
            (byte) ((headerLength >> 8) & 0xff),
            (byte) ((headerLength) & 0xff),
        });
        if (headerLength > 0) {
            result.put(header.duplicate());
        }
        if (payload != null && payload.length > 0) {
            for (ByteBuffer buffer : payload) {
                result.put(buffer.duplicate());
            }
        }
        result.putInt(payloadCrc);

        return result.array();
    }

    private int calculateLength() {
        return MIN_FRAME_SIZE + headerLength + Utilities.calculateValidBytes(payload);
    }

    public int payloadLength() {
        return length - MIN_FRAME_SIZE - headerLength;
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null) {
            return false;
        }
        if (obj == this) {
            return true;
        }
        if (obj.getClass() != getClass()) {
            return false;
        }
        SbpFrame rhs = (SbpFrame) obj;
        return new org.apache.commons.lang3.builder.EqualsBuilder()
            .append(this.length, rhs.length)
            .append(this.magicCode, rhs.magicCode)
            .append(this.operationCode, rhs.operationCode)
            .append(this.flag, rhs.flag)
            .append(this.streamId, rhs.streamId)
            .append(this.headerFormat, rhs.headerFormat)
            .append(this.headerLength, rhs.headerLength)
            .append(this.header, rhs.header)
            .append(this.payload, rhs.payload)
            .append(this.payloadCrc, rhs.payloadCrc)
            .isEquals();
    }

    @Override
    public String toString() {
        return "SbpFrame [length=" + length + ", magicCode=" + magicCode + ", operationCode=" + operationCode + ", flag="
            + flag + ", streamId=" + streamId + ", headerFormat=" + headerFormat + ", headerLength=" + headerLength
            + "]";
    }

    public boolean checkPayloadCrc(int crc) {
        return payloadCrc == crc;
    }

    public int getStreamId() {
        return streamId;
    }

    public void setStreamId(int streamId) {
        this.streamId = streamId;
    }

    public int getLength() {
        return length;
    }

    public ByteBuffer[] getPayload() {
        return payload;
    }

    public ByteBuffer getHeader() {
        return header;
    }
    public byte getMagicCode() {
        return magicCode;
    }
    public short getOperationCode() {
        return operationCode;
    }

    public byte getFlag() {
        return flag;
    }

    @Override
    public int getId() {
        return getStreamId();
    }

    @Override
    public boolean isRequest() {
        return this.flag == DEFAULT_REQUEST_FLAG;
    }

    public static SbpFrameBuilder newBuilder() {
        return new SbpFrameBuilder();
    }

    @Override
    public int refCnt() {
        if (this.byteBuf != null) {
            return this.byteBuf.refCnt();
        }
        return 0;
    }

    @Override
    public ReferenceCounted retain() {
        if (this.byteBuf != null) {
            this.byteBuf.retain();
        }
        return this;
    }

    @Override
    public ReferenceCounted retain(int increment) {
        if (this.byteBuf != null) {
            this.byteBuf.retain(increment);
        }
        return this;
    }

    @Override
    public ReferenceCounted touch() {
        return touch(null);
    }

    @Override
    public ReferenceCounted touch(Object hint) {
        return this;
    }

    @Override
    public boolean release() {
        if (this.byteBuf != null) {
            return this.byteBuf.release();
        }
        return true;
    }

    @Override
    public boolean release(int decrement) {
        if (this.byteBuf != null) {
            return this.byteBuf.release(decrement);
        }
        return true;
    }
}
