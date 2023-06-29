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

1. Launch the Placement Driver and a Range Server.
2. Use the SDK to send Append requests to the Range Server, with a record batch size of 64 MB (65536 Bytes) per request, and randomly send each request to 1, 100, or 2000 streams.
3. Adjust the number of concurrently running clients and the concurrency level of each client to achieve write rates of 500 MB/s or 1 GB/s on the Range Server. Run the test until the Range Server's resource utilization stabilizes and continue to run for some time.
4. Record the CPU and memory usage of the Range Server, the SDK request latency, and the write latency on the Range Server side during the process.

### Result
#### Elastic Stream

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

#### Kafka
| Batch size (KB) | Throughput (MB/s) | Partition | CPU Usage | Average* (ms) | P95* (ms) | P99* (ms) | P99.9* (ms) |
| :--: | :--: | :--: | :--:| :---: | :---: | :---: | :---: |
| 64   | 384  | 1    | 115% | 4.61 | 15 | 19 | 37 |
| 64   | 310  | 100  | 84% | 2.41 | 11 | 27 | 78 |
| 64   | 250  | 2000 | 73% | 1.63 | 2| 16 | 53 |

\*: E2E Latency

Note that for comparison, Kafka is tested with only one single broker. In that case, Kafka can not offer enough throughput with P99 latency as low as about 20 ms. Threrefore, more brokers are needed to achieve the same throughput.

#### Compare

| Workload  | Stream / Partition | Target P99 Latency | Elastic Stream<br />Nodes | Elastic Stream<br />Latency | Kafka<br />Nodes | Kafka<br />Latency |
| :-------: | :--: | :-----: | :----------------: | :------: | :----------------: | :---: |
| 500 MB/s  | 1    | < 20 ms | 1 (is4gen.2xlarge) | 2.075 ms | 2 (is4gen.2xlarge) | 19 ms |
| 500 MB/s  | 100  | < 20 ms | 1 (is4gen.2xlarge) | 1.976 ms | 2 (is4gen.2xlarge) | 27 ms |
| 500 MB/s  | 2000 | < 20 ms | 1 (is4gen.2xlarge) | 1.927 ms | 2 (is4gen.2xlarge) | 16 ms |
| 1000 MB/s | 1    | < 20 ms | 1 (is4gen.4xlarge) | 4.051 ms | 3 (is4gen.4xlarge) | 19 ms |
| 1000 MB/s | 100  | < 20 ms | 1 (is4gen.4xlarge) | 3.949 ms | 3 (is4gen.4xlarge) | 27 ms |
| 1000 MB/s | 2000 | < 20 ms | 1 (is4gen.4xlarge) | 3.654 ms | 4 (is4gen.4xlarge) | 16 ms |
