#!/bin/bash
set -eu
# PostgreSQL refuses a server private key readable by other users.
install -o postgres -g postgres -m 600 /run/secrets/db_key /tmp/db.key
exec /usr/local/bin/docker-entrypoint.sh postgres \
  -c hba_file=/etc/postgresql/pg_hba.conf -c ssl=on -c ssl_cert_file=/run/secrets/db_cert -c ssl_key_file=/tmp/db.key
