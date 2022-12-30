# Fire Stream

Communication library working in both ways.

There exists two implementations an encrypted one and a plain text one.

## Todo
- redo packet error stuff
- does bytes have the correct length in from_bytes_and_header?

```rust
use fire_stream::server::Connection as Server;
use fire_stream::client::Connection as Client;

```

All Naming is in the view of the final api.