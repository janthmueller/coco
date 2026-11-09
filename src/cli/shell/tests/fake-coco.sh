#!/bin/sh
set -eu
: > "$COCO_TEST_ARGS"
for argument in "$@"; do
    printf '%s\n' "$argument" >> "$COCO_TEST_ARGS"
done
for argument in "$@"; do
    case "$argument" in
        missing) printf '%s\n' "$COCO_TEST_DESTINATION"; exit 42 ;;
        cancelled) exit 130 ;;
        empty) exit 0 ;;
    esac
done
for argument in "$@"; do
    if [ "$argument" = path ]; then
        printf '%s\n' "$COCO_TEST_DESTINATION"
        exit 0
    fi
done
printf '%s\n' 'forwarded'
