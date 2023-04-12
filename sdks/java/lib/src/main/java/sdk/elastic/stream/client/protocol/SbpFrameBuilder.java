package sdk.elastic.stream.client.protocol;

import java.nio.ByteBuffer;

/**
 * SbpFrameBuilder is a builder for SbpFrame.
 */
public class SbpFrameBuilder {
    private byte magicCode = SbpFrame.DEFAULT_MAGIC_CODE;
    private short operationCode;
    private byte flag;
    private int streamId;
    private byte headerFormat = SbpFrame.DEFAULT_HEADER_FORMAT;
    private ByteBuffer header;
    private ByteBuffer[] payload;

    /**
     * Set magic code.
     *
     * @param magicCode an identifier for the presence of the SBP protocol.
     * @return SbpFrameBuilder instance.
     */
    public SbpFrameBuilder setMagicCode(byte magicCode) {
        this.magicCode = magicCode;
        return this;
    }

    /**
     * Set operation code.
     *
     * @param operationCode an identifier for the operation of this frame.
     * @return SbpFrameBuilder instance.
     */
    public SbpFrameBuilder setOperationCode(short operationCode) {
        this.operationCode = operationCode;
        return this;
    }

    /**
     * Set flag.
     *
     * @param flag an identifier for the usage of this frame.
     * @return SbpFrameBuilder instance.
     */
    public SbpFrameBuilder setFlag(byte flag) {
        this.flag = flag;
        return this;
    }

    /**
     * Set stream id.
     *
     * @param streamId A unique identifier for a request frame or a stream request frame
     * @return SbpFrameBuilder instance.
     */
    public SbpFrameBuilder setStreamId(int streamId) {
        this.streamId = streamId;
        return this;
    }

    /**
     * Set header format.
     *
     * @param headerFormat an identifier for the format of the header. If not set, the default value is 0x01.
     * @return SbpFrameBuilder instance.
     */
    public SbpFrameBuilder setHeaderFormat(byte headerFormat) {
        this.headerFormat = headerFormat;
        return this;
    }

    /**
     * Set header.
     * Make sure headerBuffer is in read mode.
     * @param headerBuffer the header of the frame.
     * @return SbpFrameBuilder instance.
     */
    public SbpFrameBuilder setHeader(ByteBuffer headerBuffer) {
        this.header = headerBuffer;
        return this;
    }

    /**
     * Set payload.
     * Make sure payload is in read mode.
     *
     * @param payload the payload of the frame.
     * @return SbpFrameBuilder instance.
     */
    public SbpFrameBuilder setPayload(ByteBuffer[] payload) {
        this.payload = payload;
        return this;
    }

    /**
     * Build SbpFrame instance.
     *
     * @return SbpFrame instance.
     */
    public SbpFrame build() {
        return new SbpFrame(magicCode, operationCode, flag, streamId, headerFormat, header, payload);
    }
}
