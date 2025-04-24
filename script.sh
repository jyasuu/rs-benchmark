# Create products index
curl -X PUT "localhost:9200/products" -H "Content-Type: application/json" -d'
{
  "mappings": {
    "properties": {
      "name": {"type": "text"},
      "category": {"type": "keyword"},
      "price": {"type": "double"},
      "stock": {"type": "integer"},
      "tags": {"type": "keyword"},
      "description": {"type": "text"},
      "created_at": {"type": "date"},
      "location": {"type": "geo_point"}
    }
  }
}'

# Index sample documents
 curl -X POST "localhost:9200/products/_bulk" -H "Content-Type: application/x-ndjson" --data-binary "@data.ndjson" | jq

 curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "match": {
      "description": "wireless Bluetooth"
    }
  }
}' | jq

curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "term": {
      "category": "Electronics"
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "range": {
      "price": {
        "gte": 50,
        "lte": 100
      }
    }
  }
}' | jq



curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "bool": {
      "must": [
        { "match": { "description": "coffee" } }
      ],
      "filter": [
        { "range": { "price": { "lte": 100 } } },
        { "term": { "stock": { "value": 25 } } }
      ],
      "must_not": [
        { "term": { "category": "Electronics" } }
      ]
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "boosting": {
      "positive": { "match": { "tags": "wireless" } },
      "negative": { "term": { "stock": 0 } },
      "negative_boost": 0.5
    }
  }
}' | jq

curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "match_phrase": {
      "description": "running shoes"
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "multi_match": {
      "query": "automatic",
      "fields": ["name", "description"]
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "exists": {
      "field": "location"
    }
  }
}' | jq

curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "ids": {
      "values": ["Oxn8Z5YB4rdtiG1naF5y", "2"]
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "geo_distance": {
      "distance": "100km",
      "location": "40.7128,-74.0060"
    }
  }
}' | jq

# First create nested mapping
curl -X PUT "localhost:9200/products/_mapping" -H "Content-Type: application/json" -d'
{
  "properties": {
    "reviews": {
      "type": "nested",
      "properties": {
        "rating": {"type": "integer"},
        "comment": {"type": "text"}
      }
    }
  }
}'

# Then query nested documents
curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "nested": {
      "path": "reviews",
      "query": {
        "range": {
          "reviews.rating": { "gte": 4 }
        }
      }
    }
  }
}'



curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "size": 0,
  "aggs": {
    "categories": {
      "terms": { "field": "category" }
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "size": 0,
  "aggs": {
    "monthly_sales": {
      "date_histogram": {
        "field": "created_at",
        "calendar_interval": "month"
      }
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "script": {
      "script": {
        "source": "doc['price'].value * doc['stock'].value > 1000"
      }
    }
  }
}' | jq


curl -X GET "localhost:9200/products/_search" -H "Content-Type: application/json" -d'
{
  "query": {
    "regexp": {
      "name": ".*shoe.*"
    }
  }
}' | jq