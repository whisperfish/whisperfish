#!/bin/bash

declare -A languages=( [ca]="Catalan" [cs]="Czech" [de]="German" [el]="Greek" [es]="Spanish" [eu]="Basque" [fi]="Finnish" [fr]="French" [hu]="Hungarian" [it]="Italian" [lt]="Lithuanian" [nb_NO]="Norwegian Bokmål" [nl_BE]="Dutch (Belgium)" [nl]="Dutch" [pl]="Polish" [pt_BR]="Portuguese (Brazil)" [pt_PT]="Portuguese (Portugal)" [ro]="Romanian" [ru]="Russian" [sl]="Slovenian" [sv]="Swedish" [tr]="Turkish" [zh_CN]="Chinese (Simplified)")

# Multiple contributors should get newlines.
declare -A languages_reverse_ignore=( [ca]="" [cs]="" [de]="" [el]="" [es]="" [eu]="" [fi]="Matti Viljanen" [fr]="" [hu]="" [it]="" [lt]="" [nb_NO]="" [nl_BE]="Ruben De Smet" [nl]="Ruben De Smet" [pl]="" [pt_BR]="" [pt_PT]="" [ro]="" [ru]="" [sl]="" [sv]="" [tr]="" [zh_CN]="" )

# These people regularly touch .ts files and will show up literally in every translation.
IGNORE=$'Gabriel Margiani\nRuben De Smet\nMatti Viljanen\nMirian Margiani\nMarkus Törnqvist\nGitlab CI translation file sync'

ABOUT=qml/pages/About.qml

PREFIX="            "

git fetch --unshallow || echo "Repository already complete"
git remote -v
git shortlog -s -n HEAD -- translations/harbour-whisperfish-nl_BE.ts

for key in "${!languages[@]}"; do
    lang=${languages[$key]}

    echo "$PREFIX""SectionHeader {";
    echo "$PREFIX    ""//: $lang ($key) language about page translation section";
    echo "$PREFIX    ""//% \"$lang translators\"";
    echo "$PREFIX    ""text: qsTrId(\"whisperfish-translators-$key\")";
    echo "$PREFIX""}";
    echo "";

    echo "$PREFIX""TextArea {";
    echo "$PREFIX""    anchors.horizontalCenter: parent.horizontalCenter";
    echo "$PREFIX""    width: parent.width";
    echo "$PREFIX""    horizontalAlignment: TextEdit.Center";
    echo "$PREFIX""    readOnly: true";
    echo "$PREFIX""    text: {";

    FIRST=true
    while IFS= read -r contributor; do
        contributor=$(echo -n $contributor | awk '{sub($1 OFS, "")}1');

        while IFS= read -r non_contributor; do
            # This check should withhold the while from running,
            # but my bash skills only know `break 2`.
            while IFS= read -r c; do
                if [ "$c" = "$contributor" ]; then
                    break 2;
                fi
            done <<< "${languages_reverse_ignore[$key]}"
            if [ "$contributor" = "$non_contributor" ] ; then
                continue 2;
            fi
        done <<< "$IGNORE"

        if [ "$FIRST" = "true" ]; then
            FIRST=false
        else
            echo " + \"\\n\" +"
        fi

        echo -n "$PREFIX        \"$contributor\""

    done <<< "`git shortlog -s -n HEAD -- translations/harbour-whisperfish-$key.ts`"
    echo ""
    echo "$PREFIX""    }";
    echo "$PREFIX""}";
    echo "";
done > TRANSLATORS.tmp

text=$(cat TRANSLATORS.tmp)
rm TRANSLATORS.tmp

start=`sed -e '/BEGIN TRANSLATORS/q' $ABOUT`

end=`sed -ne '/END TRANSLATORS/,$ p' $ABOUT`

echo "$start" > $ABOUT
echo "$text" >> $ABOUT
echo "$end" >> $ABOUT

if [ "$CI" = "true" ]; then
    if git diff --exit-code; then
        echo "No about page update needed";
    else
        echo "Committing translation contributor list update";
        git config --global user.email "whisperfish@rubdos.be"
        git config --global user.name "Gitlab CI translation file sync"
        git commit $ABOUT -m "Translation contributor list synchronisation";
        git remote add origin-ssh git@gitlab.com:whisperfish/whisperfish.git

        . .ci/load-ssh-key
        git push origin-ssh HEAD:main
    fi
fi
