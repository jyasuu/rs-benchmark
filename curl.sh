#!/bin/bash

# Script to test the rs-benchmark API endpoint

TAG_TO_QUERY="sint" # You can change this to test other tags

echo "Querying API for tag: $TAG_TO_QUERY"
curl -v "http://localhost:4444/api/postgres?tag=${TAG_TO_QUERY}" | jq

curl -v "http://localhost:4444/api/elasticsearch?tag=${TAG_TO_QUERY}" | jq

echo -e "\n\nDone."