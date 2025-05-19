import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"
import "../delegates"

ListItem {
    id: delegate
    property string date: Format.formatDate(lastMessage.timestamp, _dateFormat) // TODO Give session its own timestamp?
    property bool isGroup: model.isGroup
    property int unreadCount: 0 // TODO implement in model
    property bool isUnread: hasDraft || !model.read // TODO investigate: is this really a bool?
    property bool isNoteToSelf: SetupWorker.uuid === model.recipientUuid
    property bool isPinned: model.isPinned
    property bool isArchived: model.isArchived
    property bool isRegistered: model.isRegistered
    property bool isBlocked: model.isBlocked
    property bool hasDraft: model.draft.length > 0
    property string draft: model.draft
    property string profilePicture: model !== undefined ? (isGroup
        ? getGroupAvatar(model.groupId)
        : (recipient.status == Loader.Ready ? getRecipientAvatar(recipient.item.e164, recipient.item.uuid, recipient.item.externalId) : '')
    ) : ''
    property bool isPreviewDelivered: model.deliveryCount > 0 // TODO investigate: not updated for new message (#151, #55?)
    property bool isPreviewRead: model.readCount > 0 // TODO investigate: not updated for new message (#151, #55?)
    property bool isPreviewViewed: model.viewCount > 0 // TODO investigate: not updated for new message (#151, #55?)
    property bool isPreviewSent: hasLastMessage && model.sent // TODO cf. isPreviewReceived (#151)
    property bool isRemoteDeleted: hasLastMessage && lastMessage.remoteDeleted
    property bool hasText: lastMessage.message !== undefined && lastMessage.message !== ''
    property bool hasLastMessage: lastMessage.valid && lastMessage.messageId > 0
    property int expiringMessages: hasLastMessage && model.expiringMessageTimeout != -1
    property string name: isGroup ? model.groupName : (recipient.status == Loader.Ready ? getRecipientName(recipient.item.e164, recipient.item.externalId, recipient.item.name, true) : '')
    property string emoji: isGroup ? '' : (recipient.status == Loader.Ready ? (recipient.item.emoji != null ? recipient.item.emoji : '') : '')
    property string message: {
        var text = ""

        if (_debugMode) {
            text = "[" + model.id + "] "
        }

        if (isRemoteDeleted) {
            //: Placeholder note for a deleted message
            //% "this message was deleted"
            return text + qsTrId("whisperfish-message-deleted-note")
        }

        if (serviceMessage.active) {
           return text + "<i>" + serviceMessage.item._message + "</i>"
        }

        if (lastMessage.attachments.count > 0) {
            if (lastMessage.isVoiceNote) {
                text += "🎤 "
                if (!hasText) {
                    //: Session is a voice note
                    //% "Voice Message"
                    text += qsTrId("whisperfish-session-is-voice-note")
                }
            } else {
                text += "📎 "
                if (!hasText) {
                    //: Session contains an attachment label
                    //% "Attachment"
                    text += qsTrId("whisperfish-session-has-attachment")
                }
            }
        }

        if (hasText) {
            text += lastMessage.styledMessage
        }

        return text
    }

    signal relocateItem(int sessionId)

    property bool _debugMode: SettingsBridge.debug_mode
    property bool _labelsHighlighted: highlighted || isUnread
    property int _dateFormat: model.section === 'older' ? Formatter.DateMedium : (model.section === 'pinned' ? Formatter.Timepoint : Formatter.TimeValue)

    contentHeight: 3*Theme.fontSizeMedium+2*Theme.paddingMedium+2*Theme.paddingSmall
    menu: contextMenuComponent
    ListView.onRemove: animateRemoval(delegate)

    // Note: This is a "Rust model" and always needed.
    Message {
        id: lastMessage
        app: AppState
        messageId: model.messageId
    }

    // Note: This is a delegate, which is loaded when needed
    // and uses Message above as modelData.
    Loader {
        id: serviceMessage
        active: recipient.status == Loader.Ready && lastMessage.messageType != null
        asynchronous: true
        sourceComponent: ServiceMessageDelegate {
            modelData: lastMessage
            peerName: name
            visible: false
            enabled: false
        }
    }

    Loader {
        id: recipient
        active: !isGroup
        asynchronous: true
        sourceComponent: Recipient {
            app: AppState
            recipientId: model.recipientId
        }
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
            showInfoMark: !isRegistered || hasDraft || isNoteToSelf || isMuted || isBlocked || infoMarkEmoji !== ''
            infoMarkSource: {
                if (!isRegistered) 'image://theme/icon-s-warning'
                else if (isBlocked) 'image://theme/icon-s-blocked'
                else if (hasDraft) 'image://theme/icon-s-edit'
                else if (isNoteToSelf) 'image://theme/icon-s-retweet' // task|secure|retweet
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
                    pageStack.push(Qt.resolvedUrl("../pages/GroupProfilePage.qml"), { session: model })
                } else {
                    if (model.recipientUuid === SetupWorker.uuid) {
                        pageStack.push(Qt.resolvedUrl("../pages/ProfilePage.qml"), { session: model } )
                    } else if (recipient.status == Loader.Ready) {
                        pageStack.push(Qt.resolvedUrl("../pages/RecipientProfilePage.qml"), { session: model, recipient: recipient.item })
                    }
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
            text: (_debugMode && !isGroup ? "[" + model.recipientId + "] " : "") +
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
                top: upperLabel.bottom
            }
            height: fontMetrics.height + fontMetrics.lineSpacing + fontMetrics.descent/2
            wrapMode: Text.Wrap
            clip: true

            enableElide: Text.ElideRight
            color: highlighted ? Theme.secondaryHighlightColor :
                                 Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            font.italic: isRemoteDeleted
            plainText: (needsRichText ? cssStyle : '') + (hasDraft ?
                      //: Message preview for a saved, unsent message
                      //% "Draft: %1"
                      qsTrId("whisperfish-message-preview-draft").arg(draft) :
                      message)
            bypassLinking: true
            needsRichText: serviceMessage.active || /<(a |b>|i>|s>|span)/.test(message) // XXX Use Rust for this
            highlighted: _labelsHighlighted
            verticalAlignment: Text.AlignTop
        }

        OpacityRampEffect {
            offset: 0.8
            slope: 5
            sourceItem: lowerLabel
            enabled: lowerLabel.contentHeight > lowerLabel.height
            direction: OpacityRamp.LeftToRight
        }

        FontMetrics {
            id: fontMetrics
            font.family: lowerLabel.font.family
            font.pixelSize: lowerLabel.font.pixelSize
        }

        Row {
            id: timeLabel
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
                text: (expiringMessages ? "⏱ " : "") + date
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
