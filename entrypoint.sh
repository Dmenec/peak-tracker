#!/bin/sh
set -e

# Persistent volume is mounted at /data
# Create uploads dir there if it doesn't exist
mkdir -p /data/uploads

# Symlink /app/uploads → /data/uploads so the app writes to the volume
ln -sfn /data/uploads /app/uploads

# Point the database to the volume
export DATABASE_URL=${DATABASE_URL:-/data/datos.db}

exec ./peak-tracker
