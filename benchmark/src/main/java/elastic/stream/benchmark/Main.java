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
        ReadWriteOptions rwOptions = parseOptions(args);
        if (rwOptions.getReadTailThreadPerClient() > 0 && rwOptions.getWriteThreadPerClient() == 0) {
            throw new IllegalArgumentException("read tail thread per client must be 0 when write thread per client is 0");
        }
        Options options = new OptionsBuilder()
                .param("pmAddress", rwOptions.getPmAddress())
                .param("bodySize", String.valueOf(rwOptions.getBodySize()))
                .param("streamCount", String.valueOf(rwOptions.getStreamCount()))
                .shouldFailOnError(true)
                .threadGroups(rwOptions.getReadRandomThreadPerClient(), rwOptions.getReadTailThreadPerClient(), rwOptions.getWriteThreadPerClient())
                .threads(rwOptions.getClientCount() * (rwOptions.getReadRandomThreadPerClient() + rwOptions.getReadTailThreadPerClient() + rwOptions.getWriteThreadPerClient()))
                .include(".*" + ReadWrite.class.getSimpleName() + ".*")
                .forks(1)
                .warmupIterations(rwOptions.getWarmupIterations())
                .warmupTime(new TimeValue(rwOptions.getWarmupTimeSeconds(), TimeUnit.SECONDS))
                .measurementIterations(rwOptions.getMeasurementIterations())
                .measurementTime(new TimeValue(rwOptions.getMeasurementTimeSeconds(), TimeUnit.SECONDS))
                .mode(Mode.SampleTime)
                .timeUnit(TimeUnit.MILLISECONDS)
                .build();
        new Runner(options).run();
    }

    @Builder
    @Getter
    public static class ReadWriteOptions {
        public static final int DEFAULT_BODY_SIZE = 16 * 1024;
        public static final int DEFAULT_STREAM_COUNT = 64;
        public static final int DEFAULT_CLIENT_COUNT = 2;
        public static final int DEFAULT_READ_RANDOM_THREAD_PER_CLIENT = 1;
        public static final int DEFAULT_READ_TAIL_THREAD_PER_CLIENT = 1;
        public static final int DEFAULT_WRITE_THREAD_PER_CLIENT = 1;

        public static final int DEFAULT_WARMUP_ITERATIONS = 1;
        public static final int DEFAULT_WARMUP_TIME_SECONDS = 10;
        public static final int DEFAULT_MEASUREMENT_ITERATIONS = 1;
        public static final int DEFAULT_MEASUREMENT_TIME_SECONDS = 60;

        private String pmAddress;
        private int bodySize;
        private int streamCount;
        private int clientCount;
        private int readRandomThreadPerClient;
        private int readTailThreadPerClient;
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
        options.addOption(Option.builder("rrt")
                .longOpt("read-random-thread-per-client")
                .type(Integer.class)
                .hasArg()
                .desc("random read thread per client, default " + ReadWriteOptions.DEFAULT_READ_RANDOM_THREAD_PER_CLIENT)
                .build());
        options.addOption(Option.builder("rtt")
                .longOpt("read-tail-thread-per-client")
                .type(Integer.class)
                .hasArg()
                .desc("tail read thread per client, default " + ReadWriteOptions.DEFAULT_READ_TAIL_THREAD_PER_CLIENT)
                .build());
        options.addOption(Option.builder("wrt")
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
                    .readRandomThreadPerClient(Integer.parseInt(cmd.getOptionValue("read-random-thread-per-client", String.valueOf(ReadWriteOptions.DEFAULT_READ_RANDOM_THREAD_PER_CLIENT))))
                    .readTailThreadPerClient(Integer.parseInt(cmd.getOptionValue("read-tail-thread-per-client", String.valueOf(ReadWriteOptions.DEFAULT_READ_TAIL_THREAD_PER_CLIENT))))
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
