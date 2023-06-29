## Replication Layer
----
The replication layer of Elastic Stream copies data to multiple range servers.

### Overview
High availability requires that Elastic Stream can tolerate nodes going offline without interrupting applications. This means replicating data between range servers to ensure the data remains accessible.

### Chasing Write
