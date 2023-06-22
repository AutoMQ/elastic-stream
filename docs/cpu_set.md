# CPU-SET

Given that we are employing `Thread-Per-Core` strategy to apply run-to-complete paradigm, we need to assign each core during configuration.

Assume we have 4 cores available.

Store io_uring instance is running the polling mode. SQ_POLL thread binds to CPU-3.
IO thread binds to CPU-2
Primary data-node worker binds to CPU-1
Indexer thread, RocksDB thread-pool binds to CPU-0
