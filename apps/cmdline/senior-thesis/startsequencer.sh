#! /bin/bash
cd ../../../server
cargo run --release shard --port 8081 --public-url "http://localhost:8081" --sequencer-url "http://localhost:8082" --inbox-count 1 --outbox-count 1
