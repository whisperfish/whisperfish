import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../delegates"
import "../components"

Page {
    id: root
    objectName: "conversationPage"

    // Enable to focus the editor when the page is opened.
    // E.g. when starting a new chat.
    property bool editorFocus: false

    property bool isGroup: session.isGroup
    property bool isValid: session.valid
    property string conversationName: root.isGroup ? session.groupName : getRecipientName(recipient.e164, recipient.externalId, recipient.name, true)
    property string profilePicture: root.isGroup ? getGroupAvatar(session.groupId) : getRecipientAvatar(recipient.e164, recipient.uuid, recipient.externalId)
    property alias sessionId: session.sessionId
    property bool expiringMessages: root.isValid ? session.expiringMessageTimeout != -1 : false
    property DockedPanel activePanel: actionsPanel.open ? actionsPanel : (acceptPanel.open ? acceptPanel : panel)

    property int _selectedCount: messages.selectedCount // proxy to avoid some costly lookups
    property bool _showDeleteAll: false
    // XXX handle group.group_change like a real client
    property bool _accepted: root.isGroup || !recipient.valid ? true : recipient.accepted
    property bool _blocked: root.isGroup || !recipient.valid ? false : recipient.blocked
    property bool _showInputPanel: true
    on_ShowInputPanelChanged: maybeShowPanel()

    function maybeShowPanel() {
        if (_accepted && _showInputPanel == true && actionsPanel.open == false && acceptPanel.open == false)
            panel.show()
        else
            panel.hide()
    }

    Session {
        id: session
        app: AppState
        // sessionId is set through the property alias above.

        onValidChanged: if (valid) {
            // XXX: should probably be triggered on *session (id) change*,
            // because the validity flag might trigger multiple times
            if (session.draft && session.draft != "") {
                textInput.text = session.draft
                // XXX session.draft = ""
                SessionModel.saveDraft(sessionId, "")
            }
        }
    }

    Group {
        id: group
        app: AppState
        groupId: root.isGroup && root.isValid ? session.groupId : ""
        onValidChanged: if (valid) {
            maybeShowPanel()
            if (!!groupId) {
                pageStack.pushAttached(Qt.resolvedUrl("GroupProfilePage.qml"), { session: session })
            }
        }
    }

    Recipient {
        id: recipient
        app: AppState
        recipientId: !root.isGroup && root.isValid ? session.recipientId : -1
        onValidChanged: if (valid) {
            if (recipientId != -1) {
                maybeShowPanel()
                if (session.recipientUuid !== SetupWorker.uuid) {
                    pageStack.pushAttached(Qt.resolvedUrl("RecipientProfilePage.qml"), { session: session, recipient: recipient })
                } else {
                    pageStack.pushAttached(Qt.resolvedUrl("ProfilePage.qml"), { session: session })
                }
            }
        }
    }

    Connections {
        target: Qt.application
        onStateChanged: {
            if ((Qt.application.state === Qt.ApplicationActive) && (status === PageStatus.Active)) {
                // XXX this should be a call into the client/application state/...
                // TODO: Re-think what marking session as read means
                //SessionModel.markRead(sessionId)
                unreadMessageChecker.shouldRun = true
            }
        }
    }

    ConversationPageHeader {
        id: pageHeader
        title: (conversationName.length > 0
                ? conversationName
                : // Translated in SessionDelegate.qml
                 qsTrId("whisperfish-recipient-no-name"))
               + (expiringMessages ? "⏱" : "")
        isGroup: root.isValid && root.isGroup
        anchors.top: parent.top
        description: {
            if (!root.isValid) {
                return "";
            } else if (root.isGroup) {
                //: The number of members in a group, you included
                //% "%n member(s)"
                return qsTrId("whisperfish-group-n-members", group.member_count)
            }
            else return (
                !SettingsBridge.show_phone_number || conversationName === recipient.e164 || recipient.e164 == ''
                ? (
                    recipient.about != ''
                    ? recipient.about
                    //: The number of messages in a conversation, displayed in page header
                    //% "%n message(s)"
                    : qsTrId("whisperfish-chat-n-messages", messages.count)
                )
                : recipient.e164
            )
        }
        profilePicture: root.profilePicture

        opacity: isPortrait ? 1.0 : 0.0
        Behavior on opacity { FadeAnimator { } }
    }

    // Desired design:
    // - message view: full screen, below custom page header
    // - input field: anchored at the bottom, transparent background,
    //   visible when the view is at the bottom (latest message) and
    //   hidden while scrolling, becomes visible when scrolling down a
    //   little bit, always visible while the keyboard is open, not
    //   visible during the quick scroll animation,
    //   open while recording a voice note.
    //
    // Implementation:
    // The message view is anchored below the page header and extends
    // to the bottom of the page. It has an empty header at the bottom
    // (because it is inverted). A OpacityRampEffect hides the message
    // view's contents below the header when it is shown. This is
    // necessary because \c{clip: true} does not clip the view below
    // the header, and a DockedPanel parented to the main window does not.
    // follow the page orientation.
    // The real input field is defined outside the view, thus it is not
    // affected by the transparency effect.

    MessagesView {
        id: messages

        model: session.messages
        session: session
        recipient: recipient

        focus: true
        contentHeight: height
        Behavior on anchors.top { AnchorAnimation { } }
        anchors {
            top: isPortrait ? pageHeader.bottom : pageHeader.top;
            bottom: root.bottom
            left: parent.left;
            right: parent.right
        }
        clip: true // to prevent the view from flowing through the page header
        headerPositioning: ListView.InlineHeader
        header: Item {
            height: activePanel.height; width: messages.width
            Behavior on height { NumberAnimation { duration: 150 } }
        }

        onVerticalVelocityChanged: {
            if (verticalVelocity === 0) return
            _showInputPanel = !(verticalVelocity < 0 && !textInput.isVoiceNote)
        }
        onReplyTriggered: {
            if (_blocked || !_accepted) {
                return
            }
            _showInputPanel = true
            textInput.setQuote(index, modelData)
            textInput.forceEditorFocus(true)
        }
        onQuoteClicked: {
            // TODO use message id instead of index
            jumpToMessage(quotedData.index)
        }
        onIsSelectingChanged: {
            if (isSelecting && !selectionBlocked) _showInputPanel = true
            else actionsPanel.hide()
        }
        onSelectedCountChanged: {
            if (selectedCount > 0 && !selectionBlocked) actionsPanel.show()
            else actionsPanel.hide()
        }
        onSelectionBlockedChanged: {
            if (selectionBlocked) _showInputPanel = false
            else if (isSelecting) _showInputPanel = true
        }
        onShouldShowDeleteAll: {
            _showDeleteAll = showDeleteAll
        }

        onMovementStarted: {
            unreadMessageChecker.stillMoving = true
            unreadMessageChecker.shouldRun = true
        }

        onMovementEnded: {
            unreadMessageChecker.stillMoving = false
            unreadMessageChecker.shouldRun = true
        }

        onCountChanged: unreadMessageChecker.shouldRun = true

        Component.onCompleted: unreadMessageChecker.shouldRun = true

        Timer {
            id: unreadMessageChecker
            property int counter: 1
            property bool stillMoving: false
            property bool shouldRun: false
            running: shouldRun
                    && Qt.application.state === Qt.ApplicationActive
                    && status === PageStatus.Active
            interval: 200
            repeat: true
            onTriggered: {
                if (_blocked || !_accepted) {
                    return
                }
                if (!stillMoving) {
                    counter--
                    if (counter == 0) {
                        shouldRun = false
                        counter = 1
                    }
                }
                var unreadOrExpiring = []
                var middle = messages.width / 2
                var added = false
                for (var Y = 0; Y < height; Y += Theme.itemSizeMedium) {
                    var item = messages.itemAt(middle, messages.contentY + Y)
                    if (item !== null
                        && unreadOrExpiring[item.messageId] === undefined
                    ) {
                        // Set these in the "wrapper cache" so they won't
                        // show up again in the next iteration.
                        if (!item.messageRead) {
                            unreadOrExpiring.push(item.messageId)
                            item.messageRead = true
                            added = true
                        }
                        if (!added && item.messageExpiring === false && item.messageExpiresIn > 0) {
                            unreadOrExpiring.push(item.messageId)
                            item.messageExpiring = true
                        }
                        added = false
                    }
                }

                if (unreadOrExpiring.length > 0) {
                    console.log("Marking messages as read: " + unreadOrExpiring)
                    ClientWorker.mark_messages_read(unreadOrExpiring)

                    for (var i in unreadOrExpiring) {
                        console.log("Closing notification mid", unreadOrExpiring[i], "sid", sessionId)
                        closeMessageNotification(sessionId, unreadOrExpiring[i])
                    }
                }
            }
        }
    }

    OpacityRampEffect {
        sourceItem: messages
        direction: OpacityRamp.TopToBottom
        slope: sourceItem.height
        offset: 1-(activePanel.visibleSize/sourceItem.height)
        enabled: !sourceItem.quickScrollAnimating &&
                 !sourceItem.menuOpen
    }

    DockedPanel {
        id: panel
        background: null // transparent
        opacity: (actionsPanel.visibleSize > 0 || messages.menuOpen ||
                  messages.quickScrollAnimating) ? 0.0 : 1.0
        width: parent.width
        height: textInput.height
        open: true
        dock: Dock.Bottom
        onHeightChanged: if (open) show()

        Behavior on opacity { FadeAnimator { duration: 80 } }

        ChatTextInput {
            id: textInput
            width: parent.width
            anchors.bottom: parent.bottom
            enablePersonalizedPlaceholder: messages.count === 0 && !root.isGroup
            placeholderContactName: conversationName
            editor.focus: root.editorFocus
            showSeparator: !messages.atYEnd || quotedMessageShown
            editor.onFocusChanged: if (editor.focus) _showInputPanel = true
            dockMoving: panel.moving
            recipientIsRegistered: root.isValid && session.isRegistered // true for any group

            Component.onDestruction: {
                if(sessionId > -1 && session.draft !== text) {
                    SessionModel.saveDraft(sessionId, text)
                }
            }

            onQuotedMessageClicked: {
                // TODO use message id instead of index
                messages.jumpToMessage(index)
            }
            onSendMessage: {
                console.log(JSON.stringify(attachments))
                MessageModel.createMessage(sessionId, text, attachments, replyTo, true, isVoiceNote)
            }
            onSendTypingNotification: {
                ClientWorker.send_typing_notification(sessionId, true)
            }
            onSendTypingNotificationEnd: {
                ClientWorker.send_typing_notification(sessionId, false)
            }
        }
    }

    DockedPanel {
        id: actionsPanel
        background: null // transparent
        opacity: (messages.menuOpen || messages.quickScrollAnimating) ? 0.0 : 1.0
        width: parent.width
        height: actionsColumn.height + 2*Theme.horizontalPageMargin
        open: false
        dock: Dock.Bottom
        onOpenChanged: if (open && !textInput.isVoiceNote) _showInputPanel = true

        Behavior on opacity { FadeAnimator { duration: 80 } }

        Separator {
            opacity: messages.atYEnd ? 0.0 : Theme.opacityHigh
            color: Theme.secondaryHighlightColor
            horizontalAlignment: Qt.AlignHCenter
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            Behavior on opacity { FadeAnimator { } }
        }

        // ITEMS:
        // . = always visible
        // * = conditionally visible

        // -- CONTEXT MENU:
        // 0* resend        [if failed]
        // 1* react         [if not failed]
        // 2. copy
        // 3* forward       [if not failed]
        // 4. select · more

        // -- PANEL:
        // 1. clear selection
        // 2. copy
        // 3* info          [if only one selected]
        // 4. delete for me
        // 5. delete for all
        // 6* resend        [if at least one failed]
        // 7* transcribe

        Column {
            id: actionsColumn
            spacing: Theme.paddingLarge
            height: childrenRect.height
            anchors {
                verticalCenter: parent.verticalCenter
                left: parent.left
                right: parent.right
                leftMargin: Theme.horizontalPageMargin
                rightMargin: Theme.horizontalPageMargin
            }

            InfoHintLabel {
                id: infoLabel
                //: Info label shown while selecting messages
                //% "%n message(s) selected"
                defaultMessage: qsTrId("whisperfish-message-actions-info-label",
                                       _selectedCount)
            }

            // IMPORTANT:
            // - Both horizontal and vertical space may be very limited.
            //   There should never be more than two rows, and each row should
            //   contain at max. 4 icons at a time.
            // - Icons should always keep the same position so users can tap without looking.
            //   Entries may be hidden if they are at the sides and are seldomly used.
            //   Nothing should take the place of a hidden entry but there must not be any gaps.
            //   Entries that are conditionally unavailable should be deactivated, not hidden.

            Row {
                id: firstRow
                spacing: Theme.paddingLarge
                anchors.horizontalCenter: parent.horizontalCenter
                IconButton {
                    width: Theme.itemSizeSmall; height: width
                    icon.source: "image://theme/icon-m-clear"
                    //: Message action description, shown if one or more messages are selected
                    //% "Clear selection"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-clear-selection", _selectedCount))
                    onClicked: messages.resetSelection()
                }
                IconButton {
                    width: Theme.itemSizeSmall; height: width
                    icon.source: "../../icons/icon-m-copy.png"
                    //: Message action description
                    //% "Copy %n message(s)"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-copy", _selectedCount))
                    onClicked: messages.messageAction(messages.copySelected)
                }
                IconButton {
                    width: Theme.itemSizeSmall; height: width
                    icon.source: "image://theme/icon-m-about"
                    //: Message action description (only available if n==1)
                    //% "Show message info"
                    onPressedChanged: infoLabel.toggleHint(qsTrId("whisperfish-message-action-info"))
                    enabled: _selectedCount === 1
                    onClicked: messages.messageAction(messages.showMessageInfo)
                }

                // Show the buttons of the second row in the first row only in landscape mode.
                IconButton {
                    width: Theme.itemSizeSmall; height: width
                    icon.source: "image://theme/icon-m-delete"
                    //: Message action description
                    //% "Locally delete %n message(s)"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-delete-for-self", _selectedCount))
                    onClicked: messages.messageAction(messages.deleteSelectedForSelf)
                    enabled: root.isLandscape
                    visible: root.isLandscape
                }
                IconButton {
                    id: deleteAllPortrait
                    width: Theme.itemSizeSmall; height: width
                    icon.source: "../../icons/icon-m-delete-all.png"
                    //: Message action description
                    //% "Delete %n message(s) for all"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-delete-for-all", _selectedCount))
                    onClicked: messages.messageAction(messages.deleteSelectedForAll)
                    enabled: _showDeleteAll
                    visible: root.isLandscape
                }
            }
            Row {
                // Show the second row of buttons only in portrait mode.
                enabled: root.isPortrait
                visible: root.isPortrait
                height: root.isPortrait ? firstRow.height : 0

                spacing: Theme.paddingLarge
                anchors.horizontalCenter: parent.horizontalCenter
                IconButton {
                    width: Theme.itemSizeSmall; height: width
                    icon.source: "image://theme/icon-m-delete"
                    //: Message action description
                    //% "Locally delete %n message(s)"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-delete-for-self", _selectedCount))
                    onClicked: messages.messageAction(messages.deleteSelectedForSelf)
                }
                IconButton {
                    id: deleteAllLandscape
                    width: Theme.itemSizeSmall; height: width
                    icon.source: "../../icons/icon-m-delete-all.png"
                    //: Message action description
                    //% "Delete %n message(s) for all"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-delete-for-all", _selectedCount))
                    onClicked: messages.messageAction(messages.deleteSelectedForAll)
                    enabled: _showDeleteAll
                }

                // TODO find a way to count failed messages in the current selection
                IconButton {
                    width: visible ? Theme.itemSizeSmall : 0; height: width
                    icon.source: "image://theme/icon-m-refresh"
                    //: Message action description
                    //% "Retry sending (the) failed message(s)"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-resend", _selectedCount))
                    visible: false // TODO show if at least one message is failed
                                   // NOTE this action should be *hidden* if it is not applicable
                    onClicked: messages.messageAction(messages.resendSelected)
                }

                IconButton {
                    width: visible ? Theme.itemSizeSmall : 0; height: width
                    icon.source: "image://theme/icon-m-file-note-dark"
                    //: Message action description
                    //% "Transcribe %n message(s)"
                    onPressedChanged: infoLabel.toggleHint(
                                          qsTrId("whisperfish-message-action-resend", _selectedCount))
                    visible: dbusSpeechInterface.available
                    onClicked: messages.messageAction(messages.transcribeSelected)
                }
            }
        }
    }

    DockedPanel {
        id: acceptPanel
        background: null
        open: actionsPanel.open == false && (_blocked || !_accepted)
        opacity: open ? 1.0 : 0.0
        width: parent.width
        height: acceptColumn.height + 2*Theme.horizontalPageMargin
        dock: Dock.Bottom

        Behavior on opacity { FadeAnimator { duration: 150 } }

        Separator {
            opacity: messages.atYEnd ? 0.0 : Theme.opacityHigh
            color: Theme.secondaryHighlightColor
            horizontalAlignment: Qt.AlignHCenter
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            Behavior on opacity { FadeAnimator { } }
        }

        Column {
            id: acceptColumn
            spacing: Theme.paddingLarge
            height: childrenRect.height
            anchors {
                verticalCenter: parent.verticalCenter
                left: parent.left
                right: parent.right
                leftMargin: Theme.horizontalPageMargin
                rightMargin: Theme.horizontalPageMargin
            }

            InfoHintLabel {
                id: acceptInfoLabel
                defaultMessage: root.isGroup
                    //: Information about a pending or blocked group
                    //% "Let the group members message with you and let its members see your profile information?"
                    ? qsTrId("whisperfish-group-request-information")
                    //: Information about a pending or blocked recipient
                    //% "Let the contact message with you and let them see your profile information?"
                    : qsTrId("whisperfish-message-request-information")
                fontSizeMode: Text.FixedSize
                wrapMode: Text.Wrap
            }

            Row {
                // Show the second row of buttons only in portrait mode.
                enabled: root.isPortrait
                visible: root.isPortrait
                height: root.isPortrait ? firstRow.height : 0

                spacing: Theme.paddingLarge
                anchors.horizontalCenter: parent.horizontalCenter
                IconButton {
                    enabled: !_blocked
                    width: Theme.itemSizeSmall
                    height: width
                    icon.source: "image://theme/icon-m-cancel"
                    onClicked: {
                        root.isGroup
                            ? ClientWorker.handleGroupInvite(group.groupId, "block") // XXX
                            : ClientWorker.handleMessageRequest(recipient.recipientUuid, "block")
                            enabled: false
                        // XXX Workaround until recipient update propagates back
                        _blocked = true
                        _accepted = false
                        recipient.recipientUuid = recipient.recipientUuid
                        maybeShowPanel()
                    }
                }
                IconButton {
                    width: Theme.itemSizeSmall
                    height: width
                    icon.source: "image://theme/icon-m-accept"
                    onClicked: {
                        root.isGroup
                            ? ClientWorker.handleGroupInvite(group.groupId, "accept") // XXX
                            : ClientWorker.handleMessageRequest(recipient.recipientUuid, "accept")
                        acceptPanel.hide()
                        // XXX Workaround until recipient update propagates back
                        _blocked = false
                        _accepted = true
                        recipient.recipientUuid = recipient.recipientUuid
                        maybeShowPanel()
                    }
                }
            }
        }
    }
}
