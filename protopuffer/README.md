## Fire Protobuf

A library to encode and decode data in the protobuf format. Supports derive.

## Example
```rust
use protopuffer::{EncodeMessage, DecodeMessage, from_slice, to_vec};

#[derive(Debug, PartialEq, Eq, EncodeMessage, DecodeMessage)]
struct MyData {
	#[field(1)]
	s: String,
	#[field(5)]
	items: Vec<Item>
}

#[derive(Debug, PartialEq, Eq, EncodeMessage, DecodeMessage)]
struct Item {
	#[field(1)]
	name: String,
	#[field(2)]
	value: u32
}

// get the data as bytes
let mut data = MyData {
	s: "data".into(),
	items: vec![
		Item {
			name: "1".into(),
			value: 1
		},
		Item {
			name: "2".into(),
			value: 2
		}
	]
};

let bytes = to_vec(&mut data).unwrap();
let n_data = from_slice(&bytes).unwrap();
assert_eq!(data, n_data);
```
