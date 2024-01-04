import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"

ListItem {
    id: delegate
    property string date: Format.formatDate(model.timestamp, _dateFormat)
    property bool isGroup: model.isGroup
    property int unreadCount: 0 // TODO implement in model
    property bool isUnread: hasDraft || model.message !== undefined && !model.read // TODO investigate: is this really a bool?
    property bool isNoteToSelf: SetupWorker.uuid === model.recipientUuid
    property bool isPinned: model.isPinned
    property bool isArchived: model.isArchived
    property bool isRegistered: model.isRegistered
    property bool hasDraft: model.draft.length > 0
    property string draft: model.draft
    property string profilePicture: model !== undefined ? (isGroup
        ? getGroupAvatar(model.groupId)
        : getRecipientAvatar(model.recipientE164, model.recipientUuid)
    ) : ''
    property bool isPreviewDelivered: model.deliveryCount > 0 // TODO investigate: not updated for new message (#151, #55?)
    property bool isPreviewRead: model.readCount > 0 // TODO investigate: not updated for new message (#151, #55?)
    property bool isPreviewViewed: model.viewCount > 0 // TODO investigate: not updated for new message (#151, #55?)
    property bool isPreviewSent: model.sent // TODO cf. isPreviewReceived (#151)
    property bool hasAttachment: model.hasAttachment
    property string name: model.isGroup ? model.groupName : getRecipientName(model.recipientE164, model.recipientName, true)
    property string emoji: model.recipientEmoji
    property string message:
        (_debugMode ? "[" + model.id + "] " : "") +
        (hasAttachment
            ? (model.isVoiceNote
                ? ("🎤 " + (model.message === ''
                    //: Session is a voice note
                    //% "Voice Message"
                    ? qsTrId("whisperfish-session-is-voice-note") : '')
                )
                : ("📎 " + (model.message === ''
                    //: Session contains an attachment label
                    //% "Attachment"
                    ? qsTrId("whisperfish-session-has-attachment") : '')
                )
            )
            : ''
        ) +
        (model.remoteDeleted
            //: Placeholder note for a deleted message
            //% "this message was deleted"
            ? qsTrId("whisperfish-message-deleted-note")
            : (model.message !== undefined
                ? model.message
                : ''
            )
        )

    signal relocateItem(int sessionId)

    property bool _debugMode: SettingsBridge.debug_mode
    property bool _labelsHighlighted: highlighted || isUnread
    property int _dateFormat: model.section === 'older' ? Formatter.DateMedium : (model.section === 'pinned' ? Formatter.Timepoint : Formatter.TimeValue)

    contentHeight: 3*Theme.fontSizeMedium+2*Theme.paddingMedium+2*Theme.paddingSmall
    menu: contextMenuComponent
    ListView.onRemove: animateRemoval(delegate)

    Group {
        id: group
        app: AppState
        groupId: model.groupId ? model.groupId : -1
    }

    function remove(contentItem) {
        //: Delete all messages from session (past tense)
        //% "All messages deleted"
        contentItem.remorseAction(qsTrId("whisperfish-session-delete-all"),
            function() {
                console.log("Deleting all messages for session: " + model.id)
                SessionModel.remove(model.id)
            })
    }

    property int clickedSessionId: 0

    // QML is faster than diesel, so well have to
    // send the item relocation signal only
    // after we get the update ourselves...
    onIsArchivedChanged: {
        if(relocationActive) {
            relocateItem(model.id)
            relocationActive = false
        }
    }

    // ...but only when it's manually activated
    // to prevent scrolled-out-of-view cases. Augh.
    property bool relocationActive: false

    // FIXME after the session model is stable with row-moving instead of reinsertion
    // (https://gitlab.com/whisperfish/whisperfish/-/merge_requests/271)
    // the typing variable can be 100% declarative and in the page header.
    function sendTypingToHeader() {
        // Only ConversationPage.qml has `sessionId` property.
        if(pageStack.currentPage.sessionId == model.id) {
            var count = model.typing.length
            console.log("onTypingChanged for", model.id, ":", count, "typing");
            var typing;
            if (!model.isTyping || count === 0) {
                typing = ""
            } else {
                // XXX I really wish this was a model, or even a QStringList
                var cutpos = 0
                var numbers = []
                var names = []
                var peer = ""
                for(var i = 0; (i < 2) && (i < count); i++) {
                    peer = model.typing[i]
                    cutpos = peer.indexOf("|")
                    if(cutpos > 0) {
                        numbers[i] = peer.substr(0, cutpos)
                        names[i] = peer.substr(cutpos + 1)
                    } else {
                        numbers[i] = peer
                        names[i] = peer
                    }
                }
                if (count == 1)
                    //: Text shown when one person is typing
                    //% "%1 is typing"
                    typing = qsTrId("whisperfish-typing-1").arg(getRecipientName(numbers[0], names[0], false))
                else if (count == 2)
                    //: Text shown when two persons are typing
                    //% "%1 and %2 are typing"
                    typing = qsTrId("whisperfish-typing-2").arg(getRecipientName(numbers[0], names[0], false)).arg(getRecipientName(numbers[1], names[1], false))
                else if (count >= 3)
                    //: Text shown when three or more persons are typing
                    //% "%1 and %n others are typing"
                    typing = qsTrId("whisperfish-typing-3-plus").arg(getRecipientName(numbers[0], names[0], false)).arg(count - 1)
                else typing = ""
            }
            pageStack.currentPage.setTyping(typing)
        }
    }

    Connections {
        target: model
        onTypingChanged: sendTypingToHeader()
    }

    Component.onCompleted: sendTypingToHeader()

    function toggleReadState() {
        // TODO implement in model
        console.warn("setting read/unread is not implemented yet")
    }

    function togglePinState() {
        SessionModel.markPinned(model.id, !isPinned)
    }

    function toggleArchivedState() {
        relocationActive = true
        SessionModel.markArchived(model.id, !isArchived)
    }

    function toggleMutedState() {
        SessionModel.markMuted(model.id, !isMuted)
    }

    Item {
        anchors { fill: parent; leftMargin: Theme.horizontalPageMargin }

        ProfilePicture {
            id: profilePicContainer
            highlighted: delegate.highlighted
            labelsHighlighted: delegate._labelsHighlighted
            imageSource: profilePicture
            isNoteToSelf: delegate.isNoteToSelf
            isGroup: delegate.isGroup
            // TODO: Rework infomarks to four corners or something like that; we can currently show only one status or emoji
            showInfoMark: !isRegistered || isPinned || hasDraft || isNoteToSelf || isMuted || infoMarkEmoji !== ''
            infoMarkSource: {
                if (!isRegistered) 'image://theme/icon-s-warning'
                else if (hasDraft) 'image://theme/icon-s-edit'
                else if (isNoteToSelf) 'image://theme/icon-s-retweet' // task|secure|retweet
                else if (isPinned) 'image://theme/icon-s-high-importance'
                else if (isMuted) 'image://theme/icon-s-low-importance'
                else ''
            }
            infoMarkEmoji: isRegistered ? delegate.emoji : ""
            infoMarkRotation: {
                if (!isRegistered) 0
                else if (hasDraft) -90
                else 0
            }
            anchors {
                left: parent.left
                verticalCenter: parent.verticalCenter
            }
            onPressAndHold: delegate.openMenu()
            onClicked: {
                if (isGroup) {
                    pageStack.push(Qt.resolvedUrl("../pages/GroupProfilePage.qml"), { session: model, group: group })
                } else {
                    pageStack.push(Qt.resolvedUrl("../pages/ProfilePage.qml"), { recipientUuid: model.recipientUuid })
                }
            }
        }

        Label {
            id: upperLabel
            anchors {
                top: parent.top; topMargin: Theme.paddingMedium
                left: profilePicContainer.right; leftMargin: Theme.paddingLarge
                right: timeLabel.left; rightMargin: Theme.paddingMedium
            }
            highlighted: _labelsHighlighted
            maximumLineCount: 1
            truncationMode: TruncationMode.Fade
            text: (_debugMode && !model.isGroup ? "[" + model.recipientId + "] " : "") +
                (
                    isNoteToSelf ?
                    //: Name of the conversation with one's own number
                    //% "Note to self"
                    qsTrId("whisperfish-session-note-to-self") :
                    name
                )
        }

        LinkedEmojiLabel {
            id: lowerLabel
            enabled: false
            anchors {
                left: upperLabel.left; right: unreadBackground.left
                top: upperLabel.bottom; bottom: parent.bottom
            }
            wrapMode: Text.Wrap
            maximumLineCount: 2
            enableElide: Text.ElideRight
            color: highlighted ? Theme.secondaryHighlightColor :
                                 Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            plainText: hasDraft ?
                      //: Message preview for a saved, unsent message
                      //% "Draft: %1"
                      qsTrId("whisperfish-message-preview-draft").arg(draft) :
                      message
            highlighted: _labelsHighlighted
            verticalAlignment: Text.AlignTop
            font.italic: model.remoteDeleted
        }

        Row {
            id: timeLabel
            spacing: Theme.paddingSmall
            anchors {
                leftMargin: Theme.paddingSmall
                right: parent.right; rightMargin: Theme.horizontalPageMargin
                verticalCenter: upperLabel.verticalCenter
            }

            HighlightImage {
                source: isPreviewDelivered
                        ? "../../icons/icon-s-received.png" :
                          (isPreviewSent ? "../../icons/icon-s-sent.png" : "")
                color: Theme.primaryColor
                anchors.verticalCenter: parent.verticalCenter
                highlighted: _labelsHighlighted
                width: Theme.iconSizeSmall; height: width
            }

            Label {
                anchors.verticalCenter: parent.verticalCenter
                text: date
                highlighted: _labelsHighlighted
                font.pixelSize: Theme.fontSizeExtraSmall
                color: highlighted ? (isUnread ? Theme.highlightColor :
                                                 Theme.secondaryHighlightColor) :
                                     (isUnread ? Theme.highlightColor :
                                                 Theme.secondaryColor)
            }
        }

        Rectangle {
            id: unreadBackground
            anchors {
                leftMargin: Theme.paddingSmall
                right: parent.right; rightMargin: Theme.horizontalPageMargin
                verticalCenter: lowerLabel.verticalCenter
            }
            visible: isUnread && unreadCount > 0
            width: isUnread ? unreadLabel.width+Theme.paddingSmall : 0
            height: width
            radius: 20
            color: profilePicContainer.profileBackgroundColor
        }

        Label {
            id: unreadLabel
            anchors.centerIn: unreadBackground
            height: 1.2*Theme.fontSizeSmall; width: height
            visible: isUnread && unreadCount > 0
            text: isUnread ? (unreadCount > 0 ? unreadCount : ' ') : ''
            font.pixelSize: Theme.fontSizeExtraSmall
            minimumPixelSize: Theme.fontSizeTiny
            fontSizeMode: Text.Fit
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: Theme.highlightColor
            highlighted: _labelsHighlighted
        }

        GlassItem {
            visible: isUnread
            color: Theme.highlightColor
            falloffRadius: 0.16
            radius: 0.15
            anchors {
                left: parent.left
                leftMargin: (width / -2) - Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
        }
    }

    Component {
        id: contextMenuComponent

        ContextMenu {
            id: menu

            property bool delayedPinnedAction: false
            property bool delayedArchivedAction: false
            property bool delayedMutedAction: false

            // Trigger the actions when the menu has closed
            // so the UI actions don't overlap with
            // the menu closing animation, which results
            // in a _very_ jerky session list movement
            onClosed: {
                if (delayedPinnedAction) {
                    delayedPinnedAction = false
                    togglePinState()
                } else if (delayedArchivedAction) {
                    delayedArchivedAction = false
                    toggleArchivedState()
                } else if (delayedMutedAction) {
                    delayedMutedAction = false
                    toggleMutedState()
                }
            }

            /* MenuItem {
                text: isUnread ?
                          //: Mark conversation as 'read', even though it isn't
                          //% "Mark as read"
                          qsTrId("whisperfish-session-mark-read") :
                          //: Mark conversation as 'unread', even though it isn't
                          //% "Mark as unread"
                          qsTrId("whisperfish-session-mark-unread")
                onClicked: toggleReadState()
            } */
            MenuItem {
                text: isPinned
                        //: 'Unpin' conversation from the top of the view
                        //% "Unpin"
                      ? qsTrId("whisperfish-session-mark-unpinned")
                        //: 'Pin' conversation to the top of the view
                        //% "Pin to top"
                      : qsTrId("whisperfish-session-mark-pinned")
                // To prevent jerkiness
                onClicked: delayedPinnedAction = true
            }

            MenuItem {
                text: isMuted ?
                          //: Mark conversation as unmuted
                          //% "Unmute conversation"
                          qsTrId("whisperfish-session-mark-unmuted") :
                          //: Mark conversation as muted
                          //% "Mute conversation"
                          qsTrId("whisperfish-session-mark-muted")
                onClicked: delayedMutedAction = true
            }

            MenuItem {
                text: isArchived ?
                          //: Show archived messages again in the main page
                          //% "Restore to inbox"
                          qsTrId("whisperfish-session-mark-unarchived") :
                          //: Move the conversation to archived conversations
                          //% "Archive conversation"
                          qsTrId("whisperfish-session-mark-archived")
                onClicked: delayedArchivedAction = true
            }

            MenuItem {
                //: Delete all messages from session menu
                //% "Delete conversation"
                text: qsTrId("whisperfish-session-delete")
                onClicked: remove(delegate)
            }
        }
    }
}
