# Benchmark

## Write

Benchmark on `Append` operations.

### Platform

[i4i.2xlarge](https://aws.amazon.com/ec2/instance-types/i4i/#Product_Details)
- 8 vCPUs
- 64 GiB Memory
- 1250.00 MB/s [EBS Maximum throughput](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ebs-optimized.html#current-storage-optimized)
- Ubuntu Server 22.04 LTS

### Approach

1. Launch the Placement Manager and a Data Node.
2. Use the SDK to send Append requests to the Data Node, with a record batch size of 64 MB (65536 Bytes) per request, and randomly send each request to 1, 100, or 2000 streams.
3. Adjust the number of concurrently running clients and the concurrency level of each client to achieve write rates of 500 MB/s or 1 GB/s on the Data Node. Run the test until the Data Node's resource utilization stabilizes and continue to run for some time.
4. Record the CPU and memory usage of the Data Node, the SDK request latency, and the write latency on the Data Node side during the process.

### Result

| Batch size (KB) | Throughput (MB/s) | Stream | CPU Usage | Memory Usage (MB) | Average* (ms) | P95* (ms) | P99* (ms) | P99.9* (ms) | Average** (us) | P95** (us) | P99** (us) | P99.9** (us) |
| :--: | :--: | :--: | :--: | :--: | :---: | :---: | :---: | :---: | :--: | :--: | :--: | :--: |
| 64   | 500  | 1    | 185% | 1060 | 0.874 | 1.002 | 2.075 | 7.381 | 189  | 296  | 399  | 1803 |
| 64   | 500  | 100  | 180% | 1062 | 0.877 | 1.006 | 1.976 | 7.152 | 188  | 297  | 394  | 1694 |
| 64   | 500  | 2000 | 182% | 1069 | 0.879 | 1.011 | 1.927 | 7.176 | 191  | 299  | 399  | 1781 |
| 64   | 1000 | 1    | 275% | 1055 | 1.380 | 1.798 | 4.051 | 7.537 | 327  | 534  | 864  | 3870 |
| 64   | 1000 | 100  | 286% | 1099 | 1.322 | 1.675 | 3.949 | 7.799 | 364  | 592  | 977  | 4345 |
| 64   | 1000 | 2000 | 282% | 1065 | 1.316 | 1.669 | 3.654 | 6.693 | 359  | 589  | 1019 | 3888 |

\*: E2E Latency

\**: Server Side Latency

### Compared with [Kafka](https://github.com/apache/kafka)

| Workload  | Stream / Partition | Target P99.9 Latency | Elastic Stream<br />CPU Usage | Elastic Stream<br />Latency | Kafka<br />CPU Usage | Kafka<br />Latency |
| :-------: | :--: | :-----: | :--: | :------: | :--: | :-------: |
| 500 MB/s  | 1    | < 20 ms | 185% | 2.075 ms |      |           |
| 500 MB/s  | 100  | < 20 ms | 180% | 1.976 ms |      |           |
| 500 MB/s  | 2000 | < 20 ms | 182% | 1.927 ms |      |           |
| 1000 MB/s | 1    | < 20 ms | 275% | 4.051 ms |      |           |
| 1000 MB/s | 100  | < 20 ms | 286% | 3.949 ms |      |           |
| 1000 MB/s | 2000 | < 20 ms | 282% | 3.654 ms |      |           |
