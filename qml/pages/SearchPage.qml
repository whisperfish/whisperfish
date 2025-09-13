import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../delegates"
import "../components"

Page {
    id: root
    objectName: "searchPage"

    property int sessionId: -1
    property var sessions: null
    property bool loading: false

    property int selectedSessionId: -1

    function search(text) {
        loading = true
        ClientWorker.clearSearch()
        ClientWorker.search(searchField.text, selectedSessionId)
    }

    function resetSearch() {
        ClientWorker.clearSearch()
        searchField.text = ""
    }

    function goToMessage(targetSessionId, targetMessageId) {
        var mainPage = pageStack.find(function (page) {
            return page.objectName == "mainPage";
        })
        pageStack.replaceAbove(mainPage, Qt.resolvedUrl("../pages/ConversationPage.qml"), {
            sessionId: targetSessionId,
            targetMessageId: targetMessageId
        })
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

    Component.onCompleted: {
        if (sessionId > -1) {
            selectedSessionId = sessionId
        }
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
        height: Math.max(searchField.height, searchButton.height) + sessionCombo.height
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

        ComboBox {
            id: sessionCombo

            clip: true
            width: parent.width
            anchors.top: searchField.bottom

            property string ownAci: SetupWorker.uuid

            // If session was given, hide the session selector
            Component.onCompleted: if (sessionId > -1) {
                sessionCombo.height = 0
            }

            //: Search page, select session to search from, or all
            //% "Search from conversation"
            label: qsTrId("whisperfish-search-select-session")

            menu: ContextMenu {
                MenuItem {
                    //: Search page, search from all conversations
                    //% "All conversations"
                    text: qsTrId("whisperfish-search-from-all")
                    onClicked: {
                        resetSearch()
                        root.selectedSessionId = -1
                    }
                }
                Repeater {
                    model: sessions ? sessions.sessionNames : []
                    MenuItem {
                        property string resolvedName: modelData.isGroup
                                                    ? ''
                                                    : getRecipientName(modelData.e164, modelData.externalId, modelData.name, true)
                        property string name: modelData.isGroup ? modelData.name
                                              : resolvedName.length > 0 ? resolvedName
                                                : // Translation in SessionDelegate.qml
                                                  qsTrId("whisperfish-recipient-no-name")
                        text: sessionCombo.ownAci == modelData.aci ?
                            // Translation in SessionDelegate.qml
                            qsTrId("whisperfish-session-note-to-self") :
                            name

                        onClicked: {
                            resetSearch()
                            root.selectedSessionId = modelData.id
                        }
                    }
                }
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
                    property string maybeGroupOrChat: sessionCombo.currentIndex > 0 ? '' : (" (" + modelData.chatName + ")")
                    plainText: (
                        modelData.isOutbound
                            ? //: Name shown when replying to own messages
                              //% "You"
                              qsTrId("whisperfish-sender-name-label-outgoing")
                            : modelData.senderName
                        ) + maybeGroupOrChat
                    maximumLineCount: 1
                    wrapMode: Text.NoWrap
                    font.pixelSize: Theme.fontSizeExtraSmall
                    font.bold: true
                    linkColor: color
                    color: Qt.tint(message.highlighted ? Theme.highlightColor : Theme.primaryColor,
                                '#'+Qt.md5(modelData.isOutbound ? qsTrId("whisperfish-sender-name-label-outgoing") : modelData.senderName).substr(0, 6)+'0F')
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

            onClicked: goToMessage(modelData.sessionId, modelData.messageId)
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
