package com.automq.elasticstream.client.api;

/**
 * All stream client exceptions will list extends ElasticStreamClientException and list here.
 */
public class ElasticStreamClientException extends Exception {

    public static class FetchOutOfBoundExceptionElastic extends ElasticStreamClientException {

    }

}
