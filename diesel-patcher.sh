#!/bin/bash

# Don't nag glob doesn't match any files
shopt -s nullglob

self=$(realpath $0)
export self

help() {
    echo "Helper to create Diesel schema migrations."
    echo ""
    echo "Usage: $self [prep|apply|again]"
    echo ""
    echo "  prep    Restore schema.rs and protocol.rs to pre-patched state"
    echo "  apply   Attempt to generate updated schema and patch files"
    echo "  again   Restore your previous attempt schema and patch files"
    echo "  clean   Remove all backed up schema and patch files"
    echo "  reset   Restore schema files from git and remove temporary files"
    echo ""
    echo "Tip: After 'prep', start by reverting any unwanred changes"
    echo "in schema.rs and protocol.rs files, commit only the necessary changes."
}

run() {
    # Run the given function twice, for both schema.[rs|patch] and protocol.[rs|patch]
    $1 whisperfish-store/src schema
    $1 whisperfish-store/src/schema protocol
}

# original = "what's in git"
# vanilla  = the schema generated without applying patches
# patched  = the changes after the user had modified [schema|protocol].rs files

# Backup and restore workers #

_backup() {
    cp $1/$2.rs    $1/$2.rs.$3
    cp $1/$2.patch $1/$2.patch.$3
}

_restore() {
    cp $1/$2.rs.$3    $1/$2.rs
    cp $1/$2.patch.$3 $1/$2.patch
}

# Backup helpers #

backup_original() {
    _backup $1 $2 original
}

backup_patched() {
    _backup $1 $2 next
}

backup_vanilla() {
    _backup $1 $2 vanilla
}

# Restore helpers #

restore_original() {
    _restore $1 $2 original
}

restore_patched() {
    _restore $1 $2 next
}

restore_vanilla() {
    _restore $1 $2 vanilla
}

# Other helpers #

remove_backups() {
    for f in $1/$2.rs.* $1/$2.patch.* ; do
        rm $f
    done
}

create_patches() {
    # Manually overwrite the patch headers to minimize diff
    echo "--- a/$1/$2.rs" > $1/$2.patch
    echo "+++ b/$1/$2.rs" >> $1/$2.patch
    diff -u $1/$2.rs.vanilla $1/$2.rs | tail -n +3 >> $1/$2.patch
}

checkout() {
    git checkout --quiet $1/$2.rs $1/$2.patch
}

# Main functions (if not oneliners) #

prep() {
    run backup_original

    # Generate the unpatched schema .rs files by commenting out `patch_file = ...`
    sed -E -i 's/^patch_file/# patch_file/g' diesel.toml
    diesel --database-url /tmp/wf.db migration run
    sed -E -i 's/^# patch_file/patch_file/g' diesel.toml

    run backup_vanilla

    echo "Edit schema.rs and protocol.rs as to your liking, then run:"
    echo ""
    echo "  $self apply"
    echo ""
}

apply() {
    run backup_patched
    run create_patches

    diesel --database-url /tmp/wf.db migration run

    if test $? -eq 0; then
        echo "Schema patch files generated. Congratulations!"
        echo "To remove the temporary files created, run:"
        echo ""
        echo "  $self clean"
        echo ""
    else
        echo "Migration failed. To restore your modifications, run:"
        echo ""
        echo "  $self again"
        echo ""
        exit 1
    fi
}

# Main block #

if test ! -f whisperfish-store/src/schema/protocol.rs; then
    echo "Run this script from Whisperfish git root"
    exit 1
elif test "$1" = "prep"; then
    prep
elif test "$1" = "apply"; then
    apply
elif test "$1" = "again"; then
    run restore_patched
elif test "$1" = "reset"; then
    run remove_backups
    run checkout
elif test "$1" = "clean"; then
    run remove_backups
else
    help
fi
