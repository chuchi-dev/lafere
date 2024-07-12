
## Vec<T> & HashSet<T>

### If has a fieldnum
`repeated T vec = {fieldnum}`

### No fieldnum
```
message inner_vec {
	repeated T inner = 1;
}
```

## Enum

### no variant has a field
`int32 enum = {fieldnum}`

### some variant has a field
```
message {
	oneof inner {
		// has a field T
		T variant1 = 1;
		// no field
		bytes variant2 = 2;
		...
	}
}
```

## Option<T>

### If has a fieldnum
`T option = {fieldnum}`

### If has no fieldnum
```
message {
	T inner = 1;
}
```

## HashMap<K, V>

```
message Field {
	K key = 1;
	V value = 2;
}
```

### If has a fieldnum
`repeated Field map = {fieldnum}`

### If has no fieldnum
```
message inner_map {
	repeated Field map = {fieldnum}
}
```