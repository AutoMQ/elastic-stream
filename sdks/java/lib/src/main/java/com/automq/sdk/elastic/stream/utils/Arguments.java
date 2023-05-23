package com.automq.sdk.elastic.stream.utils;

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
