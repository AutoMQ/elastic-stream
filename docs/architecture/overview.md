## Cloud Native Design
### Object Storage
1. Unlimited Storage Capacity
2. Affordable via EC
3. SLA(Scalable + Availability + Reliability)

## Storage Achievement in NVMe SSD
1. Millions of IOPS
2. Thread-Per-Core(Run-to-Complete) architecture shifts bottleneck of IO-intensive workload from IO to CPU.
3. Scale with CPU processors
   Glommio, SPDK, io-uring, [Seastar](http://seastar.io/)

## Separation of Concern

#### Separation of Storage and Compute
Scale Independently

#### Separation of CP and AP
CP = Consistency + Partition
AP = Availability + Partition

Strong consistency while RTO tends to be zero
RPO = 0 and RTO -> 0


## Reference
1. [DFS Architecture Comparison](https://www.infoq.com/articles/dfs-architecture-comparison/)
2. [Glommio Design](https://www.datadoghq.com/blog/engineering/introducing-glommio/)
3. [The Impact of Thread-Per-Core Architecture on Application Tail Latency](https://helda.helsinki.fi//bitstream/handle/10138/313642/tpc_ancs19.pdf?sequence=1)
4. [More Than Capacity: Performance-oriented Evolution of Pangu in Alibaba](https://www.usenix.org/conference/fast23/presentation/li-qiang-deployed)
4. [SPDK: Message Passing and Concurrency Theory](https://spdk.io/doc/concurrency.html)
5. [The-5-minute-rule](https://www.allthingsdistributed.com/2012/08/the-5-minute-rule.html)
6. [Windows Azure Storage](https://www.cs.purdue.edu/homes/csjgwang/CloudNativeDB/AzureStorageSOSP11.pdf)
