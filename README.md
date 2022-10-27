# libdxcllistener

A listener to listen for new spots from a DX cluster.

The listener automatically connects to the telnet interface of a DX cluster.
Afterwards each received line is made available through a communication channel.

See `example/` folder for exemplary usage. The example `basic.rs` shows the usage when just connecting to a single cluster server. The second example `advanced.rs` shows how to connect to multiple cluster servers in parallel.


## Supported DX-Clusters

- DXSpider
- AR-Cluster
- CC Cluster
- Reverse Beacon Network