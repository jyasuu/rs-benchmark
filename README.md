# rs-benchmark

```log
Starting benchmark...
Connecting to databases...
Connections established.
Setting up database schemas...
PostgreSQL table 'documents' and FTS index checked/created.
Elasticsearch index 'documents' already exists.
Schemas ready.
Generating 1000000 documents...
Generating 1000000 documents using Faker...
  [00:03:52] [########################################] 1000000/1000000 (0s)                                                                                                                                                                   Data generation took: 232.127456959s
Inserting data into PostgreSQL...
Updating FTS vectors in PostgreSQL...
FTS vector update took: 913.090081414s, updated 1000000 potential rows
PostgreSQL insertion took: 1313.202821801s
Inserting data into Elasticsearch...
Inserting 1000000 documents into Elasticsearch in batches of 500...
  [00:03:17] [########################################] 1000000/1000000 (0s) Elasticsearch insertion complete                                                                                               Refreshing Elasticsearch index...
Elasticsearch refresh took: 2.054401343s
Elasticsearch insertion took: 199.501397341s

Running PostgreSQL benchmarks...
Query                     | Count      | Latency (ms)   
------------------------------------------------------------
database performance      | 0          | 3.6650         
search engine             | 0          | 0.5858         
distributed systems       | 0          | 0.4968         
rust programming          | 0          | 0.5598         
benchmark results         | 0          | 0.6143         
lorem ipsum dolor         | 0          | 1.0891         
quick brown fox           | 0          | 0.5284         
quos quia                 | 10         | 127541.9861    
------------------------------------------------------------
PostgreSQL Average Latency: 15943.6906ms (8 queries, 10 total results)

Running Elasticsearch benchmarks...
Query                     | Count      | Latency (ms)   
------------------------------------------------------------
database performance      | 0          | 457.1389       
search engine             | 0          | 6.9412         
distributed systems       | 0          | 7.2847         
rust programming          | 0          | 4.1661         
benchmark results         | 0          | 4.3127         
lorem ipsum dolor         | 10         | 955.8011       
quick brown fox           | 0          | 4.0211         
quos quia                 | 10         | 521.1731       
------------------------------------------------------------
Elasticsearch Average Latency: 245.1049ms (8 queries, 20 total results)

Benchmark finished.
```