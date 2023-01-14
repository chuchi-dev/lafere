/// Todo once async trait get's stabilized this shoud be refactored
/// to be a function instead of a macro
///
/// ```ignore
/// trait PacketStream {
/// 	fn timeout(&self) -> Duration;
///
/// 	async fn send(&mut self, packet: P) -> io::Result<()>;
///
/// 	async receive(&mut self) -> Result<P, PacketReadError<P::Header>>;
///
/// 	async shutdown(&mut self) -> io::Result<()>;
/// }
/// ```


macro_rules! bg_stream {
	($name:ident, $handler:ty, $bytes:ty, $cfg:ty) => {
		async fn $name<S, P>(
			mut stream: PacketStream<S, P>,
			handler: &mut $handler,
			cfg_rx: &mut watch::Receiver<$cfg>,
			mut close: &mut oneshot::Receiver<()>
		) -> Result<(), TaskError>
		where
			S: ByteStream,
			P: Packet<$bytes>
		{
			let mut should_close = false;
			let mut close_packet = None;

			let timeout = stream.timeout();
			let diff = match timeout.as_secs() {
				0..=1 => 0,
				0..=10 => 1,
				_ => 5
			};
			let mut interval = interval(timeout - Duration::from_secs(diff));
			interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

			loop {
				tokio::select!{
					packet = stream.receive(), if !should_close => {
						let r_packet = match packet {
							Ok(p) => {
								handler.send(p).await?
							},
							Err(PacketReceiverError::Io(e)) => {
								return Err(TaskError::Io(e))
							},
							Err(PacketReceiverError::Hard(e)) => {
								return Err(TaskError::Packet(e))
							},
							Err(PacketReceiverError::Soft(h, e)) => {
								handler.packet_error(h, e)?
							}
						};

						match r_packet {
							SendBack::None => {},
							SendBack::Packet(p) => {
								stream.send(p).await
									.map_err(TaskError::Io)?;
							},
							SendBack::Close => {
								should_close = true;
								let _ = handler.close();
							},
							SendBack::CloseWithPacket => {
								should_close = true;
								let packet = handler.close();
								close_packet = Some(packet);
							}
						}
					},
					Some(packet) = handler.to_send() => {
						// Todo make this not block until everything is sent
						// this can stop receiving
						stream.send(packet).await
							.map_err(TaskError::Io)?;
					},
					_ping = interval.tick(), if !should_close => {
						stream.send(handler.ping_packet()).await
							.map_err(TaskError::Io)?;
					},
					_ = &mut close, if !should_close => {
						should_close = true;
						let packet = handler.close();
						close_packet = Some(packet);
					},
					Some(cfg) = cfg_rx.recv(), if !should_close => {
						// should update configuration
						stream.stream.set_timeout(cfg.timeout);
						stream.builder.set_body_limit(cfg.body_limit);
					},
					else => {
						if let Some(packet) = close_packet.take() {
							if let Err(e) = stream.send(packet).await {
								eprintln!("error sending close packet {:?}", e);
							}
						}
						if let Err(e) = stream.shutdown().await {
							eprintln!("error shutting down {:?}", e);
						}

						return Ok(())
					}
				}

			}
		}
	}
}

macro_rules! client_bg_reconnect {
	(
		$fn:ident(
			$stream:ident,
			$bg_handler:ident,
			$cfg_rx:ident,
			$rx_close:ident,
			$recon_strat:ident,
			// return Result<PacketStream, TaskError>
			|$n_stream:ident, $cfg:ident| $block:block
		)
	) => (

		let mut stream = Some($stream);

		loop {
			// reconnect if
			let stream = match (stream.take(), &mut $recon_strat) {
				(Some(s), _) => s,
				// no recon and no stream
				// this is not possible since on the first iteration a stream
				// always exists and if the stream failes and there
				// is no recon strategy we return
				(None, None) => unreachable!(),
				(None, Some(recon)) => {
					let mut err_counter = 0;

					loop {
						let stream = (recon.inner)(err_counter).await;
						match stream {
							Ok(s) => break s,
							Err(e) => {
								eprintln!(
									"reconnect failed attempt {} {:?}",
									err_counter,
									e
								);
								err_counter += 1;
							}
						}
					}
				}
			};

			let cfg = $cfg_rx.newest();
			let stream = {
				let $n_stream = stream;
				let $cfg = cfg;

				$block
			};
			let stream = match stream {
				Ok(s) => s,
				Err(e) => {
					eprintln!("creating packetstream failed {:?}", e);
					// close since we can't reconnect
					if $recon_strat.is_none() {
						return Err(e)
					}

					continue
				}
			};

			let r = $fn(
				stream,
				&mut $bg_handler,
				&mut $cfg_rx,
				&mut $rx_close
			).await;

			match r {
				Ok(o) => return Ok(o),
				Err(e) => {
					eprintln!("fire stream client connection failed {:?}", e);
					if $recon_strat.is_none() {
						// close since we can't reconnect
						return Err(e)
					}
				}
			}

			// close all started requests because the connection failed
			$bg_handler.close_all_started();
		}
	)
}