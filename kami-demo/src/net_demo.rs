//! Multiplayer demo: server + client in one binary.
//!
//! Run server:  cargo run -p kami-demo --release -- server
//! Run client:  cargo run -p kami-demo --release -- client
//! Run client2: cargo run -p kami-demo --release -- client

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use kami_core::ipc::{Delta, Dtype, Frame, compute_delta};
use kami_knp::client::{Client, ClientEvent, ConnectionState};
use kami_knp::packet::Channel;
use kami_knp::server::{Server, ServerEvent};

const SERVER_ADDR: &str = "127.0.0.1:7777";
const TICK_RATE: u32 = 60;
const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_RATE as u64);
const ENTITY_COUNT: u32 = 4;

/// Run headless server: accept clients, broadcast position deltas at 60Hz.
pub fn run_server() {
    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let mut server = Server::bind(addr).expect("failed to bind server");
    println!("[server] listening on {}", server.local_addr().unwrap());

    // Server-authoritative positions (ENTITY_COUNT entities × 3 floats)
    let mut positions = vec![0.0f32; ENTITY_COUNT as usize * 3];
    for i in 0..ENTITY_COUNT as usize {
        positions[i * 3] = i as f32 * 3.0; // spread on X axis
    }
    let mut prev_frame: Option<Frame> = None;
    let mut tick: u32 = 0;

    println!("[server] {} entities, {TICK_RATE}Hz tick", ENTITY_COUNT);

    loop {
        let tick_start = Instant::now();

        // Poll incoming
        let events = server.poll();
        for event in events {
            match event {
                ServerEvent::ClientConnected { client_id, addr } => {
                    let entity_idx = (client_id - 1) % ENTITY_COUNT;
                    server.assign_entity(&addr, entity_idx);
                    println!(
                        "[server] client {client_id} connected from {addr}, assigned entity {entity_idx}"
                    );

                    // Send full state to new client
                    let full_frame = build_frame(&positions, tick);
                    let full_bytes = full_frame_to_bytes(&positions, tick);
                    server.send_to_addr(addr, Channel::ReliableUnordered, full_bytes);
                }
                ServerEvent::ClientData {
                    client_id,
                    channel,
                    payload,
                } => {
                    match channel {
                        Channel::Unreliable => {
                            // Client sent position update for its entity
                            if payload.len() >= 12 {
                                let entity_idx = (client_id - 1) as usize % ENTITY_COUNT as usize;
                                let x = f32::from_le_bytes(payload[0..4].try_into().unwrap());
                                let y = f32::from_le_bytes(payload[4..8].try_into().unwrap());
                                let z = f32::from_le_bytes(payload[8..12].try_into().unwrap());
                                positions[entity_idx * 3] = x;
                                positions[entity_idx * 3 + 1] = y;
                                positions[entity_idx * 3 + 2] = z;
                            }
                        }
                        Channel::ReliableOrdered => {
                            // Chat message
                            if let Ok(msg) = std::str::from_utf8(&payload) {
                                println!("[server] chat from client {client_id}: {msg}");
                                let relay = format!("[{client_id}] {msg}");
                                server.broadcast(Channel::ReliableOrdered, relay.into_bytes());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Broadcast delta to all clients
        if server.client_count() > 0 {
            let current = build_frame(&positions, tick);
            if let Some(ref prev) = prev_frame {
                if prev.n_entities == current.n_entities
                    && prev.n_columns() > 0
                    && current.n_columns() > 0
                {
                    let delta = compute_delta(prev, &current);
                    if !delta.changed_indices.is_empty() {
                        let delta_bytes = delta.to_bytes();
                        server.broadcast(Channel::Unreliable, delta_bytes);
                    }
                }
            }
            prev_frame = Some(current);
        }

        tick = tick.wrapping_add(1);

        // Fixed timestep sleep
        let elapsed = tick_start.elapsed();
        if elapsed < TICK_DURATION {
            std::thread::sleep(TICK_DURATION - elapsed);
        }

        // Status every 5 seconds
        if tick % (TICK_RATE * 5) == 0 {
            println!("[server] tick={tick}, clients={}", server.client_count());
        }
    }
}

/// Run client: connect to server, control entity, receive broadcasts.
pub fn run_client() {
    let addr: SocketAddr = SERVER_ADDR.parse().unwrap();
    let mut client = Client::connect(addr).expect("failed to connect");
    println!("[client] connecting to {SERVER_ADDR}...");

    let mut my_pos = [0.0f32; 3];
    let mut tick: u32 = 0;
    let mut positions = vec![0.0f32; ENTITY_COUNT as usize * 3];
    let mut my_entity: u32 = 0;

    loop {
        let tick_start = Instant::now();

        // Poll
        let events = client.poll();
        for event in events {
            match event {
                ClientEvent::Connected {
                    session_id,
                    client_id,
                } => {
                    my_entity = (client_id - 1) % ENTITY_COUNT;
                    println!(
                        "[client] connected! session={session_id}, client_id={client_id}, entity={my_entity}"
                    );
                }
                ClientEvent::Data { channel, payload } => {
                    match channel {
                        Channel::Unreliable => {
                            // Delta from server
                            if let Some(delta) = Delta::from_bytes(&payload) {
                                apply_delta_to_positions(&mut positions, &delta);
                            }
                        }
                        Channel::ReliableUnordered => {
                            // Full state from server
                            if payload.len() >= 8 + ENTITY_COUNT as usize * 12 {
                                parse_full_state(&payload, &mut positions);
                                my_pos = [
                                    positions[my_entity as usize * 3],
                                    positions[my_entity as usize * 3 + 1],
                                    positions[my_entity as usize * 3 + 2],
                                ];
                                println!("[client] received full state, my_pos={:?}", my_pos);
                            }
                        }
                        Channel::ReliableOrdered => {
                            if let Ok(msg) = std::str::from_utf8(&payload) {
                                println!("[chat] {msg}");
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Simulate movement: circle
        if client.state() == ConnectionState::Connected {
            let t = tick as f32 / TICK_RATE as f32;
            my_pos[0] = my_entity as f32 * 3.0 + (t * 0.5).cos() * 2.0;
            my_pos[2] = (t * 0.5).sin() * 2.0;

            // Send position to server
            let mut data = Vec::with_capacity(12);
            data.extend_from_slice(&my_pos[0].to_le_bytes());
            data.extend_from_slice(&my_pos[1].to_le_bytes());
            data.extend_from_slice(&my_pos[2].to_le_bytes());
            client.send(Channel::Unreliable, data);

            // Send chat every 5 seconds
            if tick % (TICK_RATE * 5) == 0 && tick > 0 {
                let msg = format!("hello from entity {my_entity} at tick {tick}");
                client.send(Channel::ReliableOrdered, msg.into_bytes());
            }
        }

        tick = tick.wrapping_add(1);

        let elapsed = tick_start.elapsed();
        if elapsed < TICK_DURATION {
            std::thread::sleep(TICK_DURATION - elapsed);
        }

        if tick % (TICK_RATE * 3) == 0 && client.state() == ConnectionState::Connected {
            println!(
                "[client] tick={tick}, my_pos=[{:.1}, {:.1}, {:.1}]",
                my_pos[0], my_pos[1], my_pos[2]
            );
        }
    }
}

fn build_frame(positions: &[f32], tick: u32) -> Frame {
    let n = positions.len() / 3;
    let mut frame = Frame::new(tick, n as u32);
    let data = bytemuck::cast_slice::<f32, u8>(positions).to_vec();
    frame.push_column_owned(data, Dtype::F32, 3);
    frame
}

fn full_frame_to_bytes(positions: &[f32], tick: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + positions.len() * 4);
    buf.extend_from_slice(&tick.to_le_bytes());
    buf.extend_from_slice(&(positions.len() as u32 / 3).to_le_bytes());
    buf.extend_from_slice(bytemuck::cast_slice(positions));
    buf
}

fn parse_full_state(data: &[u8], positions: &mut [f32]) {
    if data.len() < 8 {
        return;
    }
    let n_entities = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    let float_count = n_entities * 3;
    let byte_count = float_count * 4;
    if data.len() >= 8 + byte_count {
        let floats: &[f32] = bytemuck::cast_slice(&data[8..8 + byte_count]);
        let copy_len = positions.len().min(floats.len());
        positions[..copy_len].copy_from_slice(&floats[..copy_len]);
    }
}

fn apply_delta_to_positions(positions: &mut [f32], delta: &Delta) {
    if delta.n_columns() == 0 {
        return;
    }
    let col = delta.column(0);
    let col_bytes = unsafe { col.as_bytes() };
    let floats: &[f32] = bytemuck::cast_slice(col_bytes);

    for (i, &entity_idx) in delta.changed_indices.iter().enumerate() {
        let dst = entity_idx as usize * 3;
        let src = i * 3;
        if dst + 3 <= positions.len() && src + 3 <= floats.len() {
            positions[dst] = floats[src];
            positions[dst + 1] = floats[src + 1];
            positions[dst + 2] = floats[src + 2];
        }
    }
}
