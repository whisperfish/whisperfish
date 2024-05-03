import QtQuick 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../delegates"
import "../components"

Page {
    id: root
    objectName: "shareDestionationV1Page"

    property string source: ''
    property var content: ({})

    Sessions {
        id: sessions
        app: AppState
    }

    SilicaListView {
        id: sessionList
        model: sessions.sessions

        anchors {
            top: parent.top
            bottom: textInput.top
            left: parent.left
            right: parent.right
        }
        clip: true

        header: PageHeader {
            title:
                //: Title of the page to select recipients and send a shared file
                //% "Share contents"
                qsTrId("whisperfish-share-page-title")
        }
        footer: Item { width: parent.width; height: Theme.paddingMedium }

        property var recipients: ({})

        delegate: ListItem {
            id: conversation
            property bool isGroup: model.isGroup
            property string profilePicture: model !== undefined ? (isGroup
                ? getGroupAvatar(model.groupId)
                : getRecipientAvatar(recipient.e164, recipient.uuid, recipient.externalId)
            ) : ''
            property string name: model.isGroup ? model.groupName : getRecipientName(recipient.e164, recipient.externalId, recipient.name, false)
            property bool isNoteToSelf: false
            property bool selected: (sessionList.recipients.indexOf(model.id) > -1)

            highlighted: down || selected

            contentHeight: Theme.fontSizeMedium+4*Theme.paddingMedium+2*Theme.paddingSmall

            onClicked: {
                var index = 's_' + model.id
                if (index in sessionList.recipients) {
                    delete sessionList.recipients[index]
                    selected = false
                } else {
                    sessionList.recipients[index] = model
                    selected = true
                }
                textInput.enableSending = Object.keys(sessionList.recipients).length > 0
            }

            Recipient {
                id: recipient
                app: AppState
                recipientId: model.recipientId
            }

            Item {
                anchors { fill: parent; leftMargin: Theme.horizontalPageMargin }

                ProfilePicture {
                    id: profilePicContainer
                    highlighted: conversation.highlighted
                    labelsHighlighted: conversation.highlighted
                    imageSource: profilePicture
                    isGroup: conversation.isGroup
                    showInfoMark: false
                    anchors {
                        left: parent.left
                        verticalCenter: parent.verticalCenter
                    }
                    onClicked: {
                        conversation.onClicked(null)
                    }
                }

                Label {
                    id: upperLabel
                    anchors {
                        top: parent.top; topMargin: 2*Theme.paddingMedium
                        left: profilePicContainer.right; leftMargin: Theme.paddingLarge
                        right: parent.left; rightMargin: Theme.paddingMedium
                    }
                    highlighted: conversation.higlighted
                    maximumLineCount: 1
                    truncationMode: TruncationMode.Fade
                    text: isNoteToSelf ?
                            //: Name of the conversation with one's own number
                            //% "Note to self"
                            qsTrId("whisperfish-session-note-to-self") :
                            name
                            //'
                }
            }
        }
    }

    ChatTextInput {
        id: textInput
        width: parent.width
        anchors.bottom: parent.bottom
        enablePersonalizedPlaceholder: false
        showSeparator: true
        enableAttachments: false
        attachments: root.source != '' ? [ { data: root.source.replace(/^file:\/\//, ''), type: '*/*' } ] : []
        enableSending: Object.keys(sessionList.recipients).length > 0

        Component.onCompleted: {
            if ('type' in root.content) {
                switch (root.content.type) {
                    case 'text/x-url':
                        text = root.content.status
                        break;
                    case 'text/plain':
                        text = (('linkTitle' in root.content) ? root.content.linkTitle + ':\n' : '') + root.content.data
                        break;
                    case 'text/vcard':
                        /* TODO: Implement correct signal-style contact
                         * sharing. Signal sends contacts as special messages
                         * and is not able to parse vcards.
                         *
                         * This is just a temporary solution with the aditional
                         * problem, that the attached file will not show up in
                         * whisperfish anymore after a reboot due to #253 (Copy sent
                         * attachments to WF-controlled directory)
                         */
                        var vcfile = Qt.resolvedUrl(StandardPaths.temporary+'/'+Date.now()+"-"+encodeURI(root.content.name))
                        var xhr = new XMLHttpRequest()
                        xhr.open('PUT', vcfile, false)
                        xhr.send(root.content.data)
                        attachments = [ { data: vcfile.replace(/^file:\/\//, ''), type: 'text/vcard' } ]
                        break;
                }
            }
        }

        onSendMessage: {
            for (var r in sessionList.recipients) {
                var recp = sessionList.recipients[r]
                MessageModel.createMessage(recp.id, text, attachments, -1, true)
            }
            pageStack.pop()
        }
    }
}
