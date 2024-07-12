use crate::test_util::{TestWriter, WireType, Wrapper};

#[test]
fn vec() {
	TestWriter::new().cmp::<Wrapper<Vec<String>>>(Wrapper(vec![]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(1)
		.write_bytes(b"t")
		.cmp::<Wrapper<Vec<String>>>(Wrapper(vec!["t".into()]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(1)
		.write_bytes(b"t")
		.write_tag(1, WireType::Len)
		.write_len(1)
		.write_bytes(b"e")
		.cmp::<Wrapper<Vec<String>>>(Wrapper(vec!["t".into(), "e".into()]));

	TestWriter::new().cmp::<Wrapper<Vec<Vec<String>>>>(Wrapper(vec![]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(0)
		.cmp::<Wrapper<Vec<Vec<String>>>>(Wrapper(vec![vec![]]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(0)
		.write_tag(1, WireType::Len)
		.write_len(0)
		.cmp::<Wrapper<Vec<Vec<String>>>>(Wrapper(vec![vec![], vec![]]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(2)
		.write_tag(1, WireType::Len)
		.write_len(0)
		.cmp::<Wrapper<Vec<Vec<String>>>>(Wrapper(vec![vec!["".into()]]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(3)
		.write_tag(1, WireType::Len)
		.write_len(1)
		.write_bytes(b"t")
		.cmp::<Wrapper<Vec<Vec<String>>>>(Wrapper(vec![vec!["t".into()]]));
}

#[test]
fn vec_packed() {
	TestWriter::new().cmp::<Wrapper<Vec<u32>>>(Wrapper(vec![]));

	TestWriter::new()
		.write_tag(1, WireType::Varint)
		.write_varint(1)
		.cmp::<Wrapper<Vec<u32>>>(Wrapper(vec![1]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(2)
		.write_varint(1)
		.write_varint(2)
		.cmp::<Wrapper<Vec<u32>>>(Wrapper(vec![1, 2]));

	TestWriter::new().cmp::<Wrapper<Vec<Vec<u32>>>>(Wrapper(vec![]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(0)
		.cmp::<Wrapper<Vec<Vec<u32>>>>(Wrapper(vec![vec![]]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(2)
		.write_tag(1, WireType::Varint)
		.write_varint(1)
		.cmp::<Wrapper<Vec<Vec<u32>>>>(Wrapper(vec![vec![1]]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(4)
		.write_tag(1, WireType::Len)
		.write_len(2)
		.write_varint(1)
		.write_varint(2)
		.cmp::<Wrapper<Vec<Vec<u32>>>>(Wrapper(vec![vec![1, 2]]));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(4)
		.write_tag(1, WireType::Len)
		.write_len(2)
		.write_varint(1)
		.write_varint(2)
		.write_tag(1, WireType::Len)
		.write_len(2)
		.write_tag(1, WireType::Varint)
		.write_varint(1)
		.cmp::<Wrapper<Vec<Vec<u32>>>>(Wrapper(vec![vec![1, 2], vec![1]]));
}

#[test]
fn option() {
	TestWriter::new().cmp::<Wrapper<Option<String>>>(Wrapper(None));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(1)
		.write_bytes(b"t")
		.cmp::<Wrapper<Option<String>>>(Wrapper(Some("t".into())));

	eprintln!("here");
	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(0)
		.cmp::<Wrapper<Option<Option<String>>>>(Wrapper(Some(None)));

	eprintln!("here");
	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(3)
		.write_tag(1, WireType::Len)
		.write_len(1)
		.write_bytes(b"t")
		.cmp::<Wrapper<Option<Option<String>>>>(Wrapper(Some(Some(
			"t".into(),
		))));

	TestWriter::new()
		.write_tag(1, WireType::Varint)
		.write_varint(1)
		.cmp::<Wrapper<Option<u32>>>(Wrapper(Some(1)));

	TestWriter::new()
		.write_tag(1, WireType::Len)
		.write_len(2)
		.write_tag(1, WireType::Varint)
		.write_varint(1)
		.cmp::<Wrapper<Option<Vec<u32>>>>(Wrapper(Some(vec![1])));
}
