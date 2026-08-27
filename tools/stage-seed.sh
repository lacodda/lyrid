#!/bin/sh
# Fills a stand with a slice of the canon: the database and the tile pyramid.
#
# Deploying ships code; this ships data, and the two move on different clocks.
# A stand rebuilds on every release, but the slice only changes when the canon
# is imported again - which is a matter of hours and happens every few stages
# at most. Tying them together would push 119 MB across the network on every
# tag to rewrite a database with what it already holds.
#
# Neither machine has a PostgreSQL client installed: here and on the stand,
# psql and pg_restore live inside the database container. So every command
# below goes through `docker compose exec`, and the dump travels over ssh as a
# stream rather than being staged on the stand's disk first.
#
# Usage:
#   tools/stage-seed.sh [--stand pi] [--dump .local/lyrid-slice-100k.dump]
#                       [--tiles tiles] [--force] [--skip-db] [--skip-tiles]
set -eu

stand=pi@pi
remote_dir=/home/pi/lyrid
dump=.local/lyrid-slice-100k.dump
tiles=tiles
force=no
do_db=yes
do_tiles=yes

while [ $# -gt 0 ]; do
    case $1 in
        --stand) stand=$2; shift 2 ;;
        --dir) remote_dir=$2; shift 2 ;;
        --dump) dump=$2; shift 2 ;;
        --tiles) tiles=$2; shift 2 ;;
        --force) force=yes; shift ;;
        --skip-db) do_db=no; shift ;;
        --skip-tiles) do_tiles=no; shift ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "stage-seed: unknown argument: $1" >&2; exit 1 ;;
    esac
done

# Run the stand's compose file from its directory, so .env is picked up there
# and the credentials never leave the stand.
remote() {
    # shellcheck disable=SC2029 # the command is built here on purpose
    ssh "$stand" "cd $remote_dir && $*"
}

# The credentials live in the stand's .env, which the ssh session does not
# load: reading them out of the file is the only way to see them, and it keeps
# them off this machine and out of the process list here.
remote_env="user=\$(grep '^POSTGRES_USER=' .env | cut -d= -f2-); \
db=\$(grep '^POSTGRES_DB=' .env | cut -d= -f2-); db=\${db:-lyrid}"

remote_psql() {
    remote "$remote_env; docker compose -f docker-compose.prod.yml exec -T db \
        psql -U \"\$user\" -d \"\$db\" $*"
}

if [ "$do_db" = yes ]; then
    [ -f "$dump" ] || {
        echo "stage-seed: no dump at $dump" >&2
        echo "  cut one first:  lyrid slice --keep 100000  &&  pg_dump -Fc -Z6" >&2
        exit 1
    }

    # Refuse to overwrite a stand that holds something. Today the only thing
    # in there is the canon, which is reproducible; from v0.10 it also holds
    # accounts, which are not.
    artists=$(remote_psql -tAc "'select count(*) from artist'" 2>/dev/null || echo unknown)
    case $artists in
        0) ;;
        unknown)
            echo "stage-seed: cannot read the stand's database - is it up?" >&2
            exit 1 ;;
        *)
            if [ "$force" != yes ]; then
                echo "stage-seed: the stand already holds $artists artists." >&2
                echo "  restoring would replace them; pass --force if that is what you want." >&2
                exit 1
            fi
            echo "stage-seed: replacing $artists artists on the stand (--force)" ;;
    esac

    size=$(wc -c < "$dump" | tr -d ' ')
    echo "stage-seed: restoring $dump ($((size / 1024 / 1024)) MB) into $stand"

    # --clean --if-exists so a re-seed replaces the schema instead of colliding
    # with it; the migration table travels in the dump, so the server finds the
    # schema already at the revision it expects.
    #
    # The dump is piped straight into the container's stdin: writing it to the
    # stand's disk first would need the space twice and leave a stale copy
    # behind on failure.
    remote "$remote_env; docker compose -f docker-compose.prod.yml exec -T db \
        pg_restore -U \"\$user\" -d \"\$db\" --clean --if-exists --no-owner --no-privileges" \
        < "$dump"

    restored=$(remote_psql -tAc "'select count(*) from artist'")
    placed=$(remote_psql -tAc "'select count(*) from artist_position'")
    echo "stage-seed: the stand holds $restored artists, $placed of them placed"
fi

if [ "$do_tiles" = yes ]; then
    [ -d "$tiles" ] || {
        echo "stage-seed: no tile directory at $tiles" >&2
        echo "  build one first:  lyrid layout --tiles $tiles" >&2
        exit 1
    }
    [ -f "$tiles/sky.json" ] || {
        echo "stage-seed: $tiles has no sky.json - that is a tile directory's manifest" >&2
        exit 1
    }

    echo "stage-seed: copying tiles from $tiles"

    # Into the running container rather than the volume directly: the server
    # already has the volume mounted and runs as the user that must own the
    # files. Writing through a throwaway container would leave them owned by
    # root and the server unable to replace them next time.
    #
    # --no-same-owner because the tar carries this machine's uid, which means
    # nothing on the stand.
    tar -C "$tiles" -cf - . | remote "docker compose -f docker-compose.prod.yml exec -T \
        server tar -C /app/static/tiles -xf - --no-same-owner"

    count=$(remote "docker compose -f docker-compose.prod.yml exec -T \
        server sh -c 'find /app/static/tiles -type f | wc -l'" | tr -d ' \r')
    echo "stage-seed: the stand holds $count tile files"
fi

echo "stage-seed: done"
