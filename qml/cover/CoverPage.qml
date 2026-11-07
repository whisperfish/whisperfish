import QtQuick 2.2
import QtQml.Models 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"
import "../delegates"

CoverBackground {
    property bool rightToLeft: Qt.application.layoutDirection === Qt.RightToLeft

    property Sessions sessions
    property bool loaded: !!sessions

    Connections {
        target: loaded ? sessions.sessions : null
        onDataChanged: sortedModel.resortAll()
        onModelReset: sortedModel.resortAll()
    }

    Label {
        id: placeholderLabel
        visible: sessionList.count === 0
        text: "Whisperfish"
        anchors.centerIn: parent

        width: Math.min(parent.width, parent.height) * 0.8
        height: width
        font.pixelSize: Theme.fontSizeHuge
        fontSizeMode: Text.Fit
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    Label {
        id: unreadCount
        text: loaded ? sessions.unread : ""
        anchors {
            top: parent.top
            left: rightToLeft ? undefined : parent.left
            right: rightToLeft ? parent.right : undefined
            topMargin: Theme.paddingMedium
            leftMargin: rightToLeft ? undefined : Theme.paddingLarge
            rightMargin: rightToLeft ? Theme.paddingLarge : undefined
        }
        font.pixelSize: Theme.fontSizeHuge

        visible: opacity > 0.0
        opacity: loaded && sessions.unread > 0 ? 1.0 : 0.0
        Behavior on opacity { NumberAnimation {} }
    }

    Label {
        id: unreadLabel

        //: Unread messages count cover label. Code requires exact line break tag "<br/>".
        //% "Unread<br/>message(s)"
        text: qsTrId("whisperfish-cover-unread-label", loaded ? sessions.unread : 0).replace("<br/>", "\n")
        font.pixelSize: Theme.fontSizeExtraSmall
        maximumLineCount: 2
        wrapMode: Text.Wrap
        fontSizeMode: Text.HorizontalFit
        lineHeight: 0.8
        height: implicitHeight/0.8
        verticalAlignment: Text.AlignVCenter

        visible: opacity > 0.0
        opacity: loaded && sessions.unread > 0 ? 1.0 : 0.0
        Behavior on opacity { NumberAnimation {} }

        anchors {
            right: rightToLeft ? unreadCount.left : parent.right
            rightMargin: Theme.paddingMedium
            left: rightToLeft ? parent.left : unreadCount.right
            leftMargin: Theme.paddingMedium
            baseline: unreadCount.baseline
            baselineOffset: lineCount > 1 ? -implicitHeight/2 : -(height-implicitHeight)/2
        }
    }

    OpacityRampEffect {
        offset: 0.9
        slope: 10
        sourceItem: unreadLabel
        enabled: unreadLabel.contentWidth > unreadLabel.width
        direction: rightToLeft ? OpacityRamp.RightToLeft : OpacityRamp.LeftToRight
    }

    SilicaListView {
        id: sessionList
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            topMargin: Theme.paddingMedium + (loaded && sessions.unread > 0 ? unreadCount.height : Theme.paddingMedium)
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
            bottom: coverActionArea.top
            Behavior on topMargin { NumberAnimation {} }
        }

        model: sortedModel
        spacing: Theme.paddingSmall
    }

    DelegateModel {
        id: sortedModel

        property bool _sorting

        function inOrder(left, right) {
            if (right.timestamp === undefined) return true // anything precedes undefined
            if (left.timestamp === undefined) return false // undefined always yields
            return left.timestamp > right.timestamp
        }

        // Binary search within [lo, sortedItems.count) for where itemModel belongs.
        function tailPosition(itemModel, lo) {
            var mid
            var hi = sortedItems.count
            while (lo < hi) {
                mid = Math.floor(lo + (hi - lo) / 2)
                if (inOrder(itemModel, sortedItems.get(mid).model)) hi = mid
                else lo = mid + 1
            }
            return lo
        }

        // Session model is already sorted by pinned and date descending,
        // so all we have to do here is merge the two runs.
        function mergeRuns() {
            var n = sortedItems.count
            var item
            var i
            for (i = 1; i < n; i++) {
                item = sortedItems.get(i)
                if (inOrder(item.model, sortedItems.get(i - 1).model)) {
                    sortedItems.move(i, tailPosition(item.model, 0))
                }
            }
        }

        function sortToGroups() {
            if (_sorting || unsortedItems.count === 0) {
                return
            }
            _sorting = true

            if (sortedItems.count === 0) {
                unsortedItems.setGroups(0, unsortedItems.count, "sorted")
                mergeRuns()
            } else {
                var item
                while (unsortedItems.count > 0) {
                    item = unsortedItems.get(0)
                    var index = tailPosition(item.model, 0)
                    item.groups = "sorted"
                    sortedItems.move(item.sortedIndex, index)
                }
            }
            _sorting = false
        }

        function resortAll() {
            if (sortedItems.count > 0) {
                sortedItems.setGroups(0, sortedItems.count, "unsorted")
            }
        }

        items.includeByDefault: false
        filterOnGroup: "sorted"

        groups: [
            DelegateModelGroup {
                id: unsortedItems
                name: "unsorted"
                includeByDefault: true
                onChanged: sortedModel.sortToGroups()
                onCountChanged: sortedModel.sortToGroups()
            },
            DelegateModelGroup {
                id: sortedItems
                name: "sorted"
                includeByDefault: false
            }
        ]

        model: loaded ? sessions.sessions : null

        delegate: Item {
            // Only show non-archived sessions with one or more messages
            visible: !model.isArchived && model.timestamp !== undefined
            width: sessionList.width
            height: visible ? messageLabel.height + recipientLabel.height : 0

            Message {
                id: lastMessage
                app: AppState
                messageId: model.messageId
                property bool hasText: lastMessage.valid && lastMessage.messageId > -1 && lastMessage.message !== undefined && lastMessage.message !== ''
            }

            Recipient {
                id: recipient
                app: AppState
                recipientId: model.recipientId
            }

            // Note: This is a delegate, which is loaded when needed
            // and uses Message above as modelData.
            Loader {
                id: serviceMessage
                active: lastMessage.messageType != null
                asynchronous: true
                sourceComponent: ServiceMessageDelegate {
                    modelData: lastMessage
                    visible: false
                    enabled: false
                }
            }

            LinkedEmojiLabel {
                id: messageLabel
                anchors {
                    top: parent.top
                    left: parent.left
                    right: parent.right
                }
                enabled: false
                font.pixelSize: Theme.fontSizeExtraSmall
                font.italic: lastMessage.hasText && lastMessage.remoteDeleted
                color: Theme.primaryColor
                clip: true
                wrapMode: Text.NoWrap
                bypassLinking: true
                needsRichText: serviceMessage.active || /<(a |b>|i>|s>|span)/.test(lastMessage.styledMessage) // XXX Use Rust for this
                plainText: model.draft.length > 0
                           ? // Translation in SessionDelegate.qml
                             qsTrId("whisperfish-message-preview-draft").arg(model.draft)
                           : (needsRichText ? cssStyle + messageText : messageText)
                property string messageText: {
                    if (lastMessage.remoteDeleted) {
                        // SessionDelegate.qml defines this
                        return qsTrId("whisperfish-message-deleted-note")
                    }

                    if (lastMessage.messageType != null) {
                        return "<i>" + serviceMessage.item
                                       ? serviceMessage.item._message
                                       //: Placeholder for service message without message string
                                       //% "Unknown service message"
                                       : qsTrId("whisperfish-service-unknown")
                                     + "</i>"
                    }

                    var newText = ""
                    if (lastMessage.attachments.count > 0) {
                        if (lastMessage.isVoiceNote) {
                            newText += "🎤 "
                            if (!lastMessage.hasText) {
                                // SessionDelegate.qml defines this
                                newText += qsTrId("whisperfish-session-is-voice-note")
                            }
                        } else {
                            newText += "📎 "
                            if (!lastMessage.hasText) {
                                // SessionDelegate.qml defines this
                                newText += qsTrId("whisperfish-session-has-attachment")
                            }
                        }
                    }

                    if (lastMessage.hasText) {
                        newText += lastMessage.styledMessage
                    }
                    return newText
                } // end text
            }

            OpacityRampEffect {
                offset: 0.8
                slope: 5
                // XXX sourceItem: lastMessage.width > [...] ? lastMessage : null
                sourceItem: messageLabel
                direction: OpacityRamp.LeftToRight
            }

            Label {
                id: recipientLabel
                anchors {
                    top: messageLabel.bottom
                    left: parent.left
                    right: parent.right
                }
                font.pixelSize: Theme.fontSizeTiny
                color: Theme.highlightColor
                truncationMode: TruncationMode.Fade
                text: model.isGroup ? model.groupName : getRecipientName(recipient.e164, recipient.externalId, recipient.name, false)
            }
        }
    }

    OpacityRampEffect {
        offset: 0.8
        slope: 5
        sourceItem: sessionList
        direction: OpacityRamp.TopToBottom
    }

    Image {
        source: "../../icons/cover-background.png"
        anchors.centerIn: parent
        width: Math.max(parent.width, parent.height)
        height: width
        fillMode: Image.PreserveAspectFit
        opacity: 0.2
    }

    CoverActionList {
        id: coverAction
        enabled: !placeholderLabel.visible
        CoverAction {
            property string _connected: "../../icons/connected.png"
            property string _disconnected: "../../icons/disconnected.png"
            iconSource: ClientWorker.connected ? _connected : _disconnected
            onTriggered: {
                if(!SetupWorker.locked) {
                    mainWindow.activate()
                    // XXX https://gitlab.com/whisperfish/whisperfish/-/issues/481
                    // mainWindow.newMessage(PageStackAction.Immediate)
                }
            }
        }
    }
}
