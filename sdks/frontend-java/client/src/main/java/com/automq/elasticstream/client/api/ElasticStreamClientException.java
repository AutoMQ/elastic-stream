package com.automq.elasticstream.client.api;

import java.util.concurrent.ExecutionException;

/**
 * All stream client exceptions will list extends ElasticStreamClientException and list here.
 */
public class ElasticStreamClientException extends ExecutionException {
    ElasticStreamClientException(String str) {
        super(str);
    }
    public static class ConnectionTimeout extends ElasticStreamClientException {
        ConnectionTimeout() {
            super("Connection to server is rejected");
        }
    }
    public static class ConnectionReset extends ElasticStreamClientException {
        private String addr;
        ConnectionReset(String addr) {
            super("Connection to " + addr + " is reset");
            this.addr = addr;
        }
        public String getAddr() {
            return this.addr;
        }
    }
    public static class StreamNotFound extends ElasticStreamClientException {
        private long stream_id;
        StreamNotFound(long stream_id) {
            super("Stream[id={" + stream_id + "}] is not found");
            this.stream_id = stream_id;
        }
        public long getStreamId() {
            return this.stream_id;
        }
    }
    public static class BrokenChannel extends ElasticStreamClientException {
        private String err_msg;
        BrokenChannel(String err_msg) {
            super("MPSC to submit command is broken: " + err_msg);
            this.err_msg = err_msg;
        }
        public String getErrMsg() {
            return this.err_msg;
        }
    }
    public static class Internal extends ElasticStreamClientException {
        private String err_msg;
        Internal(String str) {
            super("Unexpected internal client error");
            this.err_msg = str;
        }
        public String getErrMsg() {
            return this.err_msg;
        }
    } 

    // ReplicationError
    public static class ReplicationRpcTimeout extends ElasticStreamClientException {
        ReplicationRpcTimeout() {
            super("RPC timeout");
        }
    }
    public static class ReplicationInternal extends ElasticStreamClientException {
        ReplicationInternal() {
            super("Internal client error");
        }
    }
    public static class ReplicationAlreadySealed extends ElasticStreamClientException {
        ReplicationAlreadySealed() {
            super("Range is already sealed");
        }
    }
    public static class ReplicationPreconditionRequired extends ElasticStreamClientException {
        ReplicationPreconditionRequired() {
            super("Precondition required");
        }
    }
    public static class ReplicationAlreadyClosed extends ElasticStreamClientException {
        ReplicationAlreadyClosed() {
            super("Stream is already closed");
        }
    }
    public static class ReplicationSealReplicaNotEnough extends ElasticStreamClientException {
        ReplicationSealReplicaNotEnough() {
            super("Seal replicas count is not enough");
        }
    }
    public static class ReplicationFetchOutOfRange extends ElasticStreamClientException {
        ReplicationFetchOutOfRange() {
            super("Fetch request is out of range");
        }
    }
    public static class ReplicationStreamNotExist extends ElasticStreamClientException {
        ReplicationStreamNotExist() {
            super("Stream is not exist");
        }
    }

    // ClientError
    public static class RpcClientErrorBadAddress extends ElasticStreamClientException {
        RpcClientErrorBadAddress() {
            super("Bad address");
        }
    }
    public static class RpcClientErrorBadRequest extends ElasticStreamClientException {
        RpcClientErrorBadRequest() {
            super("Bad request");
        }
    }
    public static class RpcClientErrorConnectionRefused extends ElasticStreamClientException {
        private String addr;
        RpcClientErrorConnectionRefused(String addr) {
            super("Connection to " + addr + " is refused");
            this.addr = addr;
        }
        public String getAddr() {
            return this.addr;
        }
    }
    public static class RpcClientErrorConnectTimeout extends ElasticStreamClientException {
        private String addr;
        RpcClientErrorConnectTimeout(String addr) {
            super("Timeout on connecting " + addr);
            this.addr = addr;
        }
        public String getAddr() {
            return this.addr;
        }
    }
    public static class RpcClientErrorConnectFailure extends ElasticStreamClientException {
        private String err_msg;
        RpcClientErrorConnectFailure(String err_msg) {
            super("Failed to establish TCP connection. Cause: " + err_msg);
            this.err_msg = err_msg;
        }
        public String getErrMsg() {
            return this.err_msg;
        }
    }
    public static class RpcClientErrorDisableNagleAlgorithm extends ElasticStreamClientException {
        RpcClientErrorDisableNagleAlgorithm() {
            super("Failed to disable Nagle's algorithm");
        }
    }
    public static class RpcClientErrorChannelClosing extends ElasticStreamClientException {
        private String addr;
        RpcClientErrorChannelClosing(String addr) {
            super("Channel " + addr + " is half closed");
            this.addr = addr;
        }
        public String getAddr() {
            return this.addr;
        }
    }
    public static class RpcClientErrorAppend extends ElasticStreamClientException {
        private int error_code;
        RpcClientErrorAppend(int error_code) {
            super("Append records failed with error code: " + error_code);
            this.error_code = error_code;
        }
        public int getErrorCode() {
            return this.error_code;
        }
    }
    public static class RpcClientErrorCreateRange extends ElasticStreamClientException {
        private int error_code;
        RpcClientErrorCreateRange(int error_code) {
            super("Create topic failed with error code: " + error_code);
            this.error_code = error_code;
        }
        public int getErrorCode() {
            return this.error_code;
        }
    }
    public static class RpcClientErrorServerInternal extends ElasticStreamClientException {
        RpcClientErrorServerInternal() {
            super("Server internal error");
        }
    }
    public static class RpcClientErrorClientInternal extends ElasticStreamClientException {
        RpcClientErrorClientInternal() {
            super("Client internal error");
        }
    }
    public static class RpcClientErrorRpcTimeout extends ElasticStreamClientException {
        private long timeout_millis;
        RpcClientErrorRpcTimeout(long timeout_millis) {
            super("Client fails to receive response from server within " + timeout_millis + " ms");
            this.timeout_millis = timeout_millis;
        }
        public long getTimeoutMillis() {
            return this.timeout_millis;
        }
    }
}
