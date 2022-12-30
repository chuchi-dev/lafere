
This document tries to describe the internal architecture used for this crate.



# Communication stack?

## AsyncReadWrite
The AsyncReadWrite is a type that implements tokio::AsyncRead and
tokio::AsyncWrite this represent the lowest level. It handles byte slices.

## Connection
A Connection builds upon the AsyncReadWrite and provides ways to read and write
packets.

## Stream
A sequence of packets

## Session