## 0.3.3 - 2023-12-26
- add a way to get data from a built server

## 0.3.2 - 2023-12-25
- add new api to make a request to a server without going over any network stack.
Allowing to just pass bytes (useful for testing)

## 0.3.1 - 2023-12-23
- add tracing

## 0.3.0 - 2023-10-31
- Bump msrv to 1.67
- update fire-crypto 0.4
- update fire-stream to 0.4

## 0.2.6 - 2023-04-21
- Fix success flag not getting set correctly

## 0.2.5 - 2023-04-21
- add ServerConfig and expose it in Data

## 0.2.4 - 2023-01-19
- relax trait bound on register_req

## 0.2.3 - 2023-01-19
- add a feature connection which can disabled to not include tokio

## 0.2.2 - 2023-01-19
- remove action macro

## 0.2.1 - 2023-01-19
- add fire-protobuf support
- add add `#[derive(Action)]`

## 0.2.0 - 2023-01-16
- update to fire-stream version
- change some trait signatures
- add codegen for IntoMessage, FromMessage & api