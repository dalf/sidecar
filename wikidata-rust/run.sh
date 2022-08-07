#!/bin/sh
lbzcat -n 7 /data/wikidata/src/latest-all.json.bz2 | ./target/release/wikidata
