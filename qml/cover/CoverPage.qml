import QtQuick 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"
import "../delegates"

CoverBackground {
    property bool rightToLeft: Qt.application.layoutDirection === Qt.RightToLeft

    Sessions {
        id: sessions
        app: AppState
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
        text: sessions.unread
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
        opacity: sessions.unread > 0 ? 1.0 : 0.0
        Behavior on opacity { NumberAnimation {} }
    }

    Label {
        id: unreadLabel

        //: Unread messages count cover label. Code requires exact line break tag "<br/>".
        //% "Unread<br/>message(s)"
        text: qsTrId("whisperfish-cover-unread-label", sessions.unread).replace("<br/>", "\n")
        font.pixelSize: Theme.fontSizeExtraSmall
        maximumLineCount: 2
        wrapMode: Text.Wrap
        fontSizeMode: Text.HorizontalFit
        lineHeight: 0.8
        height: implicitHeight/0.8
        verticalAlignment: Text.AlignVCenter

        visible: opacity > 0.0
        opacity: sessions.unread > 0 ? 1.0 : 0.0
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
            topMargin: Theme.paddingMedium + (sessions.unread > 0 ? unreadCount.height : Theme.paddingMedium)
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
            bottom: coverActionArea.top
            Behavior on topMargin { NumberAnimation {} }
        }

        // XXX Maybe we can use a delegate model to resort without pinning.
        //     Pins don't make a lot of sense in this context
        model: sessions.sessions
        spacing: Theme.paddingSmall

        delegate: Item {
            enabled: !model.isArchived
            visible: enabled
            width: sessionList.width
            height: enabled ? messageLabel.height + recipientLabel.height : 0

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
                       return "<i>" + serviceMessage.item._message + "</i>"
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
