# DX Cluster Listener

A listener to listen for new spots from a DX cluster.

The listener automatically connects to the telnet interface of a DX cluster.
Afterwards each received line is made available through a communication channel.
Note that sending arbitrary strings or commands to the server is not supported.

See `example/` folder for exemplary usage.
The example `basic.rs` shows the usage when just connecting to a single cluster server.
The second example `advanced.rs` shows how to connect to multiple cluster servers in parallel.


## Supported DX-Clusters

The following four cluster server implementations are more or less tested and can be used to listen for spots:

- DXSpider
- AR-Cluster
- CC Cluster
- Reverse Beacon Network (RBN)