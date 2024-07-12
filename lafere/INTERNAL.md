
This document tries to describe the internal architecture used for this crate.

When a connection get's established a background task gets started which
manages packets and send it to the right receiver





## Overview of how a request is sent
first the client user calls `Client::request(req)` which will send the request
to the background task.
The background task will then set the relevant header fields like Flags and id
and then convert it to bytes.
After that the bytes will be sent via the stream.

The background task of the server receives the bytes and convert them into a
request. The request gets then sent to the server user which will receive
the request with the call `Server::receive()`.
The server user can now send a response back. Itself gets then passed back
to the background task, where it will be converted to bytes and then sent.

The background task of the client receives the bytes and converts them into a
response. That is then sent to client user and will end the future on
`Client::request(req)`.