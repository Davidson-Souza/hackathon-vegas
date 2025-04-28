#!/bin/bash
# This script is used to run end-to-end tests for the project.

# Usage: ./e2e.sh

set -euo pipefail
set -o posix

root_api_url="http://127.0.0.1:8080"

echo "Running end-to-end tests..."

echo -n "Listing all available lockers..."
# list all available lockers
lockers=$(curl -X GET \
  --silent \
  -H "accept: application/json" \
  -H "Content-Type: application/json" \
  "$root_api_url/lockers")

# check if a locker is available
available_locker=$(echo "$lockers" | jq -r '.data.[] | select(.state == "available") | .id' | head -n 1)
if [ -z "$available_locker" ]; then
  echo "No available lockers found."
  exit 1
fi

echo "(Done)"

echo -n "Using locker $available_locker..."
# start using that locker
response=$(curl -X GET \
  --silent \
  -H "accept: application/json" \
  -H "Content-Type: application/json" \
  "$root_api_url/use_locker/$available_locker")

# check the response. It should contain: the locker id, the signature and start_time. And the error should be null.
if [ "$(echo "$response" | jq -r '.error')" != "null" ]; then
  echo "Error: $(echo "$response" | jq -r '.error')"
  exit 1
fi

echo "(Done)"

echo -n "Checking locker state..."
# check if the locker is now in use
response=$(curl -X GET \
  --silent \
  -H "accept: application/json" \
  -H "Content-Type: application/json" \
  "$root_api_url/lockers/$available_locker")

if [ "$(echo "$response" | jq -r '.data.state')" != "in_use" ]; then
  echo "Error: Locker $available_locker is not in use."
  exit 1
fi

echo "(Done)"

echo -n "Paying for locker $available_locker..."
# stop using the locker
response=$(curl -X GET \
  --silent \
  -H "accept: application/json" \
  -H "Content-Type: application/json" \
  "$root_api_url/pay_for_usage/$available_locker")

if [ "$(echo "$response" | jq -r '.error')" != "null" ]; then
  echo "Error: $(echo "$response" | jq -r '.error')"
  exit 1
fi

echo "(Done)"

payment_hash=$(echo "$response" | jq -r '.data.invoice.payment_hash')

echo -n "Asking for payment receipt..."
# ask for a payment receipt
response=$(curl -X GET \
  --silent \
  -H "accept: application/json" \
  -H "Content-Type: application/json" \
  "$root_api_url/payment_receipt/$payment_hash")

if [ "$(echo "$response" | jq -r '.error')" != "null" ]; then
  echo "Error: $(echo "$response" | jq -r '.error')"
  exit 1
fi

echo "(Done)"

echo -n "Checking if the locker is now available..."
# check if the locker is now available
response=$(curl -X GET \
  --silent \
  -H "accept: application/json" \
  -H "Content-Type: application/json" \
  "$root_api_url/lockers/$available_locker")
  
if [ "$(echo "$response" | jq -r '.data.state')" != "available" ]; then
  echo "Error: Locker $available_locker is not available."
  exit 1
fi

echo "(Done)"
