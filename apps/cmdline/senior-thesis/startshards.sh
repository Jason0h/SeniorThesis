#! /bin/bash
cd ../../../server
rm -rf epoch_log.txt && rm -rf persist-outbox/ && cargo run --release sequencer --port 8082 --shard-count 1
