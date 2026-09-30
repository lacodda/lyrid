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
# A canon that grows by a few tables does not need the whole database
# replaced: `--tables` restores only the rows of the named tables, emptying them
# first, and leaves every other table -- the accounts above all -- as it is.
# The tables must already exist on the stand, which a deploy of the release
# that adds them has done by running its migrations.
#
# Usage:
#   tools/stage-seed.sh [--stand pi] [--dump .local/lyrid-slice-100k.dump]
#                       [--tiles tiles] [--force] [--skip-db] [--skip-tiles]
#                       [--tables label,label_artist,label_year]
set -eu

stand=pi@pi
remote_dir=/home/pi/lyrid
dump=.local/lyrid-slice-100k.dump
tiles=tiles
force=no
do_db=yes
do_tiles=yes
tables=

while [ $# -gt 0 ]; do
    case $1 in
        --stand) stand=$2; shift 2 ;;
        --dir) remote_dir=$2; shift 2 ;;
        --dump) dump=$2; shift 2 ;;
        --tiles) tiles=$2; shift 2 ;;
        --force) force=yes; shift ;;
        --skip-db) do_db=no; shift ;;
        --skip-tiles) do_tiles=no; shift ;;
        --tables) tables=$2; shift 2 ;;
        -h|--help) sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
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

if [ "$do_db" = yes ] && [ -n "$tables" ]; then
    [ -f "$dump" ] || { echo "stage-seed: no dump at $dump" >&2; exit 1; }

    # Table names go into SQL and onto a command line, so only the plain
    # lower_snake_case every table of this schema uses is let through.
    case $tables in
        *[!a-z0-9_,]*|,*|*,|*,,*)
            echo "stage-seed: --tables takes comma-separated table names, got: $tables" >&2
            exit 1 ;;
    esac

    # Only the named tables' rows, in the order the dump holds them: pg_dump
    # sorts table data so a referenced table loads before the one pointing
    # at it, which is what keeps the foreign keys satisfied row by row.
    list=$(mktemp)
    trap 'rm -f "$list"' EXIT
    names=$(printf '%s' "$tables" | tr ',' ' ')
    toc=$(docker compose exec -T db pg_restore -l < "$dump")
    for table in $names; do
        printf '%s\n' "$toc" | grep -qE "TABLE DATA public $table " || {
            echo "stage-seed: the dump holds no data for $table" >&2
            exit 1
        }
    done
    printf '%s\n' "$toc" | grep -E "TABLE DATA public ($(printf '%s' "$tables" | tr ',' '|')) " > "$list"

    echo "stage-seed: replacing the rows of $tables on $stand"
    # Emptied in one statement and without CASCADE: the named tables may
    # reference each other, and a table outside the list that points at one of
    # them makes the truncate fail rather than silently emptying it too.
    remote_psql -c "'TRUNCATE $(printf '%s' "$tables" | sed 's/,/, /g')'"

    # pg_restore reads a list from a file, not from a pipe it is also reading
    # the dump from, so the list is written into the container first.
    remote "docker compose -f docker-compose.prod.yml exec -T db sh -c 'cat > /tmp/stage-seed.list'" < "$list"
    remote "$remote_env; docker compose -f docker-compose.prod.yml exec -T db \
        pg_restore -U \"\$user\" -d \"\$db\" --data-only --no-owner --no-privileges -L /tmp/stage-seed.list" \
        < "$dump"

    for table in $names; do
        rows=$(remote_psql -tAc "'select count(*) from $table'")
        echo "stage-seed: the stand holds $rows rows in $table"
    done
    do_db=no
fi

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
        echo "  cut one first:  lyrid tiles --out $tiles" >&2
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
    #
    # The old cut is emptied first. Unpacking over it would leave every file
    # the new cut does not write -- a level the new pyramid stops short of, a
    # tile of a region now empty -- and the stand would serve two skies at
    # once. Emptied from inside the container for the same ownership reason.
    remote "docker compose -f docker-compose.prod.yml exec -T \
        server sh -c 'find /app/static/tiles -mindepth 1 -delete'"
    tar -C "$tiles" -cf - . | remote "docker compose -f docker-compose.prod.yml exec -T \
        server tar -C /app/static/tiles -xf - --no-same-owner"

    count=$(remote "docker compose -f docker-compose.prod.yml exec -T \
        server sh -c 'find /app/static/tiles -type f | wc -l'" | tr -d ' \r')
    echo "stage-seed: the stand holds $count tile files"
fi

echo "stage-seed: done"
