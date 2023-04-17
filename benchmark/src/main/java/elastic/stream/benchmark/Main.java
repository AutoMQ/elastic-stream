package elastic.stream.benchmark;

import elastic.stream.benchmark.jmh.ReadWrite;
import lombok.Builder;
import lombok.Getter;
import org.apache.commons.cli.CommandLine;
import org.apache.commons.cli.CommandLineParser;
import org.apache.commons.cli.DefaultParser;
import org.apache.commons.cli.HelpFormatter;
import org.apache.commons.cli.Option;
import org.apache.commons.cli.ParseException;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.runner.Runner;
import org.openjdk.jmh.runner.RunnerException;
import org.openjdk.jmh.runner.options.Options;
import org.openjdk.jmh.runner.options.OptionsBuilder;
import org.openjdk.jmh.runner.options.TimeValue;

import java.util.concurrent.TimeUnit;

/**
 * @author ningyu
 */
public class Main {
    public static void main(String[] args) throws RunnerException {
        ReadWriteOptions readWriteOptions = parseOptions(args);
        Options options = new OptionsBuilder()
                .param("pmAddress", readWriteOptions.getPmAddress())
                .param("bodySize", String.valueOf(readWriteOptions.getBodySize()))
                .param("streamCount", String.valueOf(readWriteOptions.getStreamCount()))
                .shouldFailOnError(true)
                .threadGroups(readWriteOptions.getReadThreadPerClient(), readWriteOptions.getWriteThreadPerClient())
                .threads(readWriteOptions.getClientCount() * (readWriteOptions.getReadThreadPerClient() + readWriteOptions.getWriteThreadPerClient()))
                .include(".*" + ReadWrite.class.getSimpleName() + ".*")
                .forks(1)
                .warmupIterations(readWriteOptions.getWarmupIterations())
                .warmupTime(new TimeValue(readWriteOptions.getWarmupTimeSeconds(), TimeUnit.SECONDS))
                .measurementIterations(readWriteOptions.getMeasurementIterations())
                .measurementTime(new TimeValue(readWriteOptions.getMeasurementTimeSeconds(), TimeUnit.SECONDS))
                .mode(Mode.SampleTime)
                .timeUnit(TimeUnit.MILLISECONDS)
                .build();
        new Runner(options).run();
    }

    @Builder
    @Getter
    public static class ReadWriteOptions {
        public static final int DEFAULT_BODY_SIZE = 1024 * 1024;
        public static final int DEFAULT_STREAM_COUNT = 64;
        public static final int DEFAULT_CLIENT_COUNT = 2;
        public static final int DEFAULT_READ_THREAD_PER_CLIENT = 1;
        public static final int DEFAULT_WRITE_THREAD_PER_CLIENT = 1;

        public static final int DEFAULT_WARMUP_ITERATIONS = 1;
        public static final int DEFAULT_WARMUP_TIME_SECONDS = 10;
        public static final int DEFAULT_MEASUREMENT_ITERATIONS = 1;
        public static final int DEFAULT_MEASUREMENT_TIME_SECONDS = 60;

        private String pmAddress;
        private int bodySize;
        private int streamCount;
        private int clientCount;
        private int readThreadPerClient;
        private int writeThreadPerClient;

        private int warmupIterations;
        private int warmupTimeSeconds;
        private int measurementIterations;
        private int measurementTimeSeconds;
    }

    private static ReadWriteOptions parseOptions(String[] args) {
        org.apache.commons.cli.Options options = new org.apache.commons.cli.Options();
        options.addOption("h", "help", false, "print this message");
        options.addOption(Option.builder()
                .longOpt("pm-addr")
                .required()
                .type(String.class)
                .hasArg()
                .desc("pm address")
                .build());
        options.addOption(Option.builder()
                .longOpt("body-size")
                .type(Integer.class)
                .hasArg()
                .desc("body size in bytes, default " + ReadWriteOptions.DEFAULT_BODY_SIZE)
                .build());
        options.addOption(Option.builder()
                .longOpt("stream-count")
                .type(Integer.class)
                .hasArg()
                .desc("stream count, default " + ReadWriteOptions.DEFAULT_STREAM_COUNT)
                .build());
        options.addOption(Option.builder("c")
                .longOpt("client-count")
                .type(Integer.class)
                .hasArg()
                .desc("client count, default " + ReadWriteOptions.DEFAULT_CLIENT_COUNT)
                .build());
        options.addOption(Option.builder("r")
                .longOpt("read-thread-per-client")
                .type(Integer.class)
                .hasArg()
                .desc("read thread per client, default " + ReadWriteOptions.DEFAULT_READ_THREAD_PER_CLIENT)
                .build());
        options.addOption(Option.builder("w")
                .longOpt("write-thread-per-client")
                .type(Integer.class)
                .hasArg()
                .desc("write thread per client, default " + ReadWriteOptions.DEFAULT_WRITE_THREAD_PER_CLIENT)
                .build());
        options.addOption(Option.builder("wi")
                .longOpt("warmup-iterations")
                .type(Integer.class)
                .hasArg()
                .desc("warmup iterations, default " + ReadWriteOptions.DEFAULT_WARMUP_ITERATIONS)
                .build());
        options.addOption(Option.builder("wt")
                .longOpt("warmup-time")
                .type(Integer.class)
                .hasArg()
                .desc("warmup time in seconds, default " + ReadWriteOptions.DEFAULT_WARMUP_TIME_SECONDS)
                .build());
        options.addOption(Option.builder("mi")
                .longOpt("measurement-iterations")
                .type(Integer.class)
                .hasArg()
                .desc("measurement iterations, default " + ReadWriteOptions.DEFAULT_MEASUREMENT_ITERATIONS)
                .build());
        options.addOption(Option.builder("mt")
                .longOpt("measurement-time")
                .type(Integer.class)
                .hasArg()
                .desc("measurement time in seconds, default " + ReadWriteOptions.DEFAULT_MEASUREMENT_TIME_SECONDS)
                .build());

        CommandLineParser parser = new DefaultParser();
        try {
            CommandLine cmd = parser.parse(options, args);

            return ReadWriteOptions.builder()
                    .pmAddress(cmd.getOptionValue("pm-addr"))
                    .bodySize(Integer.parseInt(cmd.getOptionValue("body-size", String.valueOf(ReadWriteOptions.DEFAULT_BODY_SIZE))))
                    .streamCount(Integer.parseInt(cmd.getOptionValue("stream-count", String.valueOf(ReadWriteOptions.DEFAULT_STREAM_COUNT))))
                    .clientCount(Integer.parseInt(cmd.getOptionValue("client-count", String.valueOf(ReadWriteOptions.DEFAULT_CLIENT_COUNT))))
                    .readThreadPerClient(Integer.parseInt(cmd.getOptionValue("read-thread-per-client", String.valueOf(ReadWriteOptions.DEFAULT_READ_THREAD_PER_CLIENT))))
                    .writeThreadPerClient(Integer.parseInt(cmd.getOptionValue("write-thread-per-client", String.valueOf(ReadWriteOptions.DEFAULT_WRITE_THREAD_PER_CLIENT))))
                    .warmupIterations(Integer.parseInt(cmd.getOptionValue("warmup-iterations", String.valueOf(ReadWriteOptions.DEFAULT_WARMUP_ITERATIONS))))
                    .warmupTimeSeconds(Integer.parseInt(cmd.getOptionValue("warmup-time", String.valueOf(ReadWriteOptions.DEFAULT_WARMUP_TIME_SECONDS))))
                    .measurementIterations(Integer.parseInt(cmd.getOptionValue("measurement-iterations", String.valueOf(ReadWriteOptions.DEFAULT_MEASUREMENT_ITERATIONS))))
                    .measurementTimeSeconds(Integer.parseInt(cmd.getOptionValue("measurement-time", String.valueOf(ReadWriteOptions.DEFAULT_MEASUREMENT_TIME_SECONDS))))
                    .build();
        } catch (ParseException e) {
            System.err.println(e.getMessage());
            printHelp(options);
            System.exit(1);
        }

        return null;
    }

    private static void printHelp(org.apache.commons.cli.Options options) {
        HelpFormatter formatter = new HelpFormatter();
        formatter.printHelp("benchmark", options);
    }
}
