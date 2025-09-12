import QtQuick 2.6
import Sailfish.Silica 1.0
import "../delegates"
import "../components"

Page {
    id: root
    objectName: "searchPage"

    property bool loading: false

    function search(text) {
        loading = true
        ClientWorker.clearSearch()
        ClientWorker.search(searchField.text)
    }

    Connections {
        target: ClientWorker
        onSearchResultsChanged: loading = false
    }

    PageHeader {
        id: pageHeader
        //: Search page title
        //% "Message search"
        title: qsTrId("whisperfish-search-title")
    }
    Component.onDestruction: {
        ClientWorker.clearSearch()
    }
    Item {
        id: searchHeader
        anchors {
            top: pageHeader.bottom
            left: parent.left
            right: parent.right
        }
        height: Math.max(searchField.height, searchButton.height)
        TextField {
            id: searchField

            anchors {
                top: parent.top
                left: parent.left
                right: searchButton.left
            }
            //: Search field default text
            //% "Search messages by text"
            label: qsTrId("whisperfish-search-field-label")
            //: Search field description
            //% "%n match(es)"
            description: qsTrId("whisperfish-search-field-desc", results.count)
            acceptableInput: text.length > 2
            EnterKey.onClicked: if (acceptableInput) {
                search(searchField.text)
            }
        }
        BackgroundItem {
            id: searchButton
            enabled: searchField.acceptableInput
            width: Theme.iconSizeMedium
            height: Theme.iconSizeMedium
            anchors {
                top: parent.top
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
            }
            IconButton {
                enabled: searchField.acceptableInput
                anchors.centerIn: parent
                icon.source: "image://theme/icon-m-search?" + (pressed ? Theme.highlightColor : Theme.primaryColor)
                onClicked: search(searchField.text)
            }
        }
    }

    SilicaListView {
        id: results

        anchors {
            top: searchHeader.bottom
            left: root.left
            right: root.right
            bottom: parent.bottom
        }
        clip: true
        model: ClientWorker.searchResults

        delegate: ListItem {
            id: message

            width: parent.width
            contentHeight: content.height + Theme.paddingMedium * 4

            RoundedRect {
                radius: Theme.paddingLarge
                anchors {
                    fill: parent
                    topMargin: Theme.paddingMedium
                    leftMargin: (modelData.isOutbound ? 5 * Theme.paddingLarge : Theme.paddingLarge)
                    rightMargin: (modelData.isOutbound ? Theme.paddingLarge : 5 * Theme.paddingLarge)
                }
                roundedCorners: modelData.isOutbound ? bottomLeft | topRight : bottomRight | topLeft
                color: parent.pressed ? Theme.highlightBackgroundColor : Theme.secondaryColor
                opacity: parent.pressed ?
                             (modelData.isOutbound ? 0.7*Theme.opacityFaint : 1.0*Theme.opacityFaint) :
                             (modelData.isOutbound ? 0.4*Theme.opacityFaint : 0.8*Theme.opacityFaint)
            }

            Column {
                id: content
                anchors {
                    top: parent.top
                    left: parent.left
                    right: parent.right
                    topMargin: Theme.paddingMedium * 2
                    leftMargin: (modelData.isOutbound ? 5 * Theme.paddingLarge : Theme.paddingLarge) + Theme.paddingMedium
                    rightMargin: (modelData.isOutbound ? Theme.paddingLarge : 5 * Theme.paddingLarge) + Theme.paddingMedium
                }
                LinkedEmojiLabel {
                    id: senderNameLabel

                    horizontalAlignment: modelData.isOutgoing ? Text.AlignRight : Text.AlignLeft
                    highlighted: message.highlighted
                    plainText: (modelData.isOutbound
                            ? //: Name shown when replying to own messages
                                //% "You"
                                qsTrId("whisperfish-sender-name-label-outgoing")
                                : modelData.senderName) +
                                (modelData.groupName != "" ? " - " + modelData.groupName : "")
                    maximumLineCount: 1
                    wrapMode: Text.NoWrap
                    font.pixelSize: Theme.fontSizeExtraSmall
                    font.bold: true
                    linkColor: color
                    color: Qt.tint(message.highlighted ? Theme.highlightColor : Theme.primaryColor,
                                '#'+Qt.md5(modelData.senderName).substr(0, 6)+'0F')
                    defaultLinkActions: false
                }

                LinkedEmojiLabel {
                    id: messageLabel

                    visible: true
                    plainText: cssStyle + modelData.text
                    bypassLinking: true
                    needsRichText: true
                    wrapMode: Text.Wrap
                    anchors { left: parent.left; right: parent.right }
                    horizontalAlignment: emojiOnly ? Text.AlignHCenter :
                                                    (modelData.isOutbound ? Text.AlignRight : Text.AlignLeft) // TODO make configurable
                    color: modelData.isOutbound ? Theme.highlightColor : Theme.primaryColor
                    linkColor: message.highlighted ? Theme.secondaryHighlightColor :
                                            Theme.secondaryColor
                    enableCounts: true
                    emojiOnlyThreshold: 5 // treat long messages as text
                    font.pixelSize: emojiOnly ?
                                        (emojiCount <= 2 ? 1.5*Theme.fontSizeLarge :
                                                        1.0*Theme.fontSizeLarge) :
                                        Theme.fontSizeSmall // TODO make configurable
                }
                Label {
                    anchors {
                        left: parent.left
                        right: parent.right
                    }
                    text: String(modelData.timestamp).substring(0, 16)
                    horizontalAlignment: modelData.isOutbound ? Text.AlignRight : Text.AlignLeft // TODO make configurable
                    font.pixelSize: Theme.fontSizeExtraSmall // TODO make configurable
                    color: modelData.isOutbound ?
                               (highlighted ? Theme.secondaryHighlightColor :
                                              Theme.secondaryHighlightColor) :
                               (highlighted ? Theme.secondaryHighlightColor :
                                              Theme.secondaryColor)
                }
            }
            Item {
                height: 2 * Theme.paddingMedium
            }

            onClicked: pageStack.replace(
                Qt.resolvedUrl("ConversationPage.qml"), {
                    sessionId: modelData.sessionId,
                    targetMessageId: modelData.messageId
                })
        }

        ViewPlaceholder {
            enabled: results.count == 0 && !loading
            //: Search results placeholder text
            //% "No messages"
            text: qsTrId("whisperfish-search-results-label")
        }

        BusyIndicator {
            id: spinner
            size: BusyIndicatorSize.Large
            anchors.centerIn: parent
            running: loading
        }
    }
}
