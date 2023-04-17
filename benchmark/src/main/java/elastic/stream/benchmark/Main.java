package elastic.stream.benchmark;

import elastic.stream.benchmark.jmh.ReadWrite;
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
        Options options = new OptionsBuilder()
                .param("pmAddress", "192.168.123.128:2378")
                .param("bodySize", "1024")
                .param("streamCount", "64")
                .shouldFailOnError(true)
                .threadGroups(3, 1)
                .include(".*" + ReadWrite.class.getSimpleName() + ".*")
                .forks(1)
                .warmupIterations(1)
                .warmupTime(new TimeValue(10, TimeUnit.SECONDS))
                .measurementIterations(1)
                .measurementTime(new TimeValue(60, TimeUnit.SECONDS))
                .mode(Mode.SampleTime)
                .timeUnit(TimeUnit.MILLISECONDS)
                .build();
        new Runner(options).run();
    }
}