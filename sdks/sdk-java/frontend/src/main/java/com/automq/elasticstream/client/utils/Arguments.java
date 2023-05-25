package com.automq.elasticstream.client.utils;

public class Arguments {

    public static void check(boolean checkResult, String errorMessage) {
        if (!checkResult) {
            throw new IllegalArgumentException(errorMessage);
        }
    }

    public static void isNotNull(Object obj, String errorMessage)  {
        if (obj == null) {
            throw new IllegalArgumentException(errorMessage);
        }
    }

}
