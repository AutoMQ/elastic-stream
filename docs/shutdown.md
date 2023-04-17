# Shutdown Procedure

Data node register handlers for `SIGTERM` and `SIGHUP` using `ctrlc` crate. Once shutdown signal is caught, it broadcasts the shutdown attempt to all `Worker`s.

On receiving shutdown signal, `Worker` will stop accepting new TCP connections and send `GOAWAY` frame to each existing connections. Data-node assumes that front-end clients will complete their sessions at their earliest convenient time and close connections thereafter.

Once all connections are closed, data-node would drop IO-task channel, triggering shutdown procedure of IO module. Once all IO tasks are served, notify indexer to flush memtable. 