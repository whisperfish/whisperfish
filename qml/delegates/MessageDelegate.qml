// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"
import "../components/message"

ListItem {
    id: root
    width: parent.width
    contentHeight: contentContainer.height
    highlighted: down || menuOpen || replyArea.pressed || isSelected
    _backgroundColor: "transparent"
    hidden: !!(isSelected && listView.hideSelected)
    enabled: !modelData.remoteDeleted

    // REQUIRED PROPERTIES
    property QtObject modelData
    // TODO the quoted message should be a notifyable object from a model
    // TODO we need a way to get a valid index from a message id
    //      (we must rely on the message's id instead of its index, as the latter may change)
    // TODO 'attachments' is expected as a list of objects: [{data: path, type: mimetype}, ...]
    // required properties: message, source, outgoing, attachments, AND id, index
    // The parent view can specify a signal to be emitted when
    // the user wants to reply to the delegate's message.
    // Signal signature: \c{replySignal(var index, var modelData)}.
    property var replySignal

    // The parent view can specify a signal to be emitted when
    // the user clicked on the quoted message.
    // Signal signature: \c{quoteClickedSignal(var clickedIndex, var quotedData)}.
    property var quoteClickedSignal

    // DATA PROPERTIES: bound to modelData, proxied because the delegate is not used directly
    property ListView listView: ListView.view
    property int index: hasData ? modelData.index : -1

    property string fullMessageText: ""

    readonly property string _message: fullMessageText !== "" ? fullMessageText : (hasData ? (modelData.styledMessage.trim() !== "" ? modelData.styledMessage.trim() : (isAudioTranscription ? ("🎤 <i>" + attachmentsLoader.audioTranscription + "</i>") : "") ) : '')
    readonly property bool isAudioTranscription: attachmentsLoader.audioTranscription !== "" && hasData && modelData.styledMessage.trim() === ""
    // TODO implement shared locations (show a map etc.; is probably not an attachment)

    Loader {
        id: sender
        active: showSender
        asynchronous: true
        sourceComponent: Component {
            Recipient {
                app: AppState
                recipientId: modelData.senderRecipientId
            }
        }
    }

    // TODO: Don't query recipient name for each message separately
    readonly property string contactName: (showSender && sender.loaded) ? getRecipientName(sender.item.e164, sender.item.externalId, sender.item.name, false) : "..."
    // sender.loaded && sender.item.valid has a problem when sender is not yet loaded.
    readonly property string contactNameValid: !showSender || (sender.loaded ? sender.item.valid : false)

    // All children are placed inside a bubble, positioned left or right for
    // incoming/outbound messages. The bubble extends slightly over the contents.
    default property alias delegateContents: contentColumn.data
    property real contentPadding: 2*Theme.paddingMedium
    property real delegateContentWidth: isExpanded ? _expandedWidth : _unexpandedWidth
    property real _expandedWidth: width - 2*Theme.horizontalPageMargin // page width
    property real _unexpandedWidth: hasAttachments ?
                                        maxMessageWidth :
                                        Math.min(Math.max(metrics.width+messageLabel.emojiCount *
                                                          messageLabel.font.pixelSize, infoRow.minContentWidth) +
                                                 Theme.paddingMedium, maxMessageWidth)
    property real maxMessageWidth: parent.width - 6*Theme.horizontalPageMargin
    property real minMessageWidth: Math.max(showSender ? senderNameLabel.implicitWidth : 0,
                                            showQuotedMessage ? quoteItem.implicitWidth : 0,
                                            showExpand ? maxMessageWidth : infoRow.minContentWidth)
    property int shortenThreshold: 600 // in characters
    property int extraPageTreshold: 1500 // in characters
    property bool expandExtraPage: showExpand && (_message.length > extraPageTreshold)
    property real backgroundCornerRadius: Theme.paddingLarge

    property bool showSender: isInGroup && !isOutbound
    property bool showQuotedMessage: hasQuotedMessage && !isRemoteDeleted
    property bool showExpand: !isEmpty && !isRemoteDeleted && _message.length > shortenThreshold

    readonly property bool hasData: modelData !== null && modelData !== undefined
    readonly property bool hasReactions: hasData && modelData.reactions > 0
    readonly property bool hasQuotedMessage: !!modelData.quotedMessageId && modelData.quotedMessageId != -1 && !isRemoteDeleted
    readonly property bool hasAttachments: hasData && modelData.attachments > 0 && !isRemoteDeleted
    readonly property bool hasText: hasData && _message !== ''
    readonly property bool unidentifiedSender: modelData.unidentifiedSender !== undefined ? modelData.unidentifiedSender : true
    readonly property bool isOutbound: hasData && (modelData.outgoing === true)
    readonly property bool isInGroup: session.isGroup
    readonly property bool isEmpty: !hasText && !hasAttachments
    readonly property bool isRemoteDeleted: hasData && ((isSelected && listView.appearDeleted) || modelData.remoteDeleted)
    property bool isExpanded: false
    property bool isSelected: listView !== null && listView.selectedMessages[modelData.id] !== undefined

    Loader {
        id: reactions
        active: hasReactions
        asynchronous: true
        sourceComponent: Component {
            GroupedReactions {
                app: AppState
                messageId: modelData.id
            }
        }
    }

    function handleExternalPressAndHold(mouse) {
        if (openMenuOnPressAndHold) openMenu()
        else pressAndHold(mouse) // propagate
    }

    onClicked: {
        // selection is handled in messagesView
        if (listView.isSelecting || !showExpand) return
        if (expandExtraPage) {
            // TODO Cache the page object, so we can return to the
            // same scroll position where the user left the page.
            // It is not possible to re-use the returned object from pageStack.push().
            pageStack.push("../pages/ExpandedMessagePage.qml", { 'modelData': modelData, 'contactName': contactName })
        } else {
            isExpanded = !isExpanded
            // We make sure the list item is visible immediately
            // after changing the state. If omitted, closing a very
            // long delegate would leave the view to be positionend
            // somewhere off - possibly destroyed, and expansionTimer
            // would not trigger.
            listView.positionViewAtIndex(index, ListView.Contain)
            expansionTimer.start()
        }
    }

    TextMetrics {
        id: metrics
        text: messageLabel.plainText
        font: messageLabel.font
    }

    Timer {
        // This timer waits a moment until we can be mostly certain that the expansion is finished.
        // It then positions the delegate at the top of the page, i.e. ListView.End because the view
        // is inverted. Without the timer, the view would jump around.
        // TODO There is a some flickering which can't be avoided this way. (We need a better solution.)
        // TODO Sometimes jumping back fails...
        id: expansionTimer
        interval: isEmpty ? 0 : 5*_message.length/shortenThreshold
        onTriggered: {
            listView.positionViewAtIndex(index, ListView.End)
        }
    }

    RoundedRect {
        id: background
        radius: backgroundCornerRadius
        anchors { fill: contentContainer; margins: contentPadding/3 }
        roundedCorners: isOutbound ? bottomLeft | topRight : bottomRight | topLeft
        color: (down || replyArea.pressed || isSelected) ? Theme.highlightBackgroundColor : Theme.secondaryColor
        opacity: (down || replyArea.pressed || isSelected) ?
                     (isOutbound ? 0.7*Theme.opacityFaint : 1.0*Theme.opacityFaint) :
                     (isOutbound ? 0.4*Theme.opacityFaint : 0.8*Theme.opacityFaint)
    }

    Loader {
        id: replyArea
        property bool pressed: item && item.down
        asynchronous: true
        anchors { bottom: parent.bottom; top: parent.top }
        width: parent.width/2
        sourceComponent: Component {
            ReplyArea { enabled: root.enabled && !listView.isSelecting }
        }
    }

    Column {
        id: contentContainer
        // IMPORTANT Never use 'parent.width' in this content container!
        // This breaks width calculations here and in derived items.
        // Always use delegateContentWidth instead.
        padding: contentPadding
        spacing: 0
        anchors {
            // The text should be aligned with other page elements by having the default side margins.
            // The bubble should extend a little bit over the margins.
            top: parent.top
            rightMargin: Theme.horizontalPageMargin - contentPadding
            leftMargin: Theme.horizontalPageMargin - contentPadding
        }

        SenderNameLabel {
            id: senderNameLabel
            visible: showSender
            source: contactNameValid ?
                      contactName :
                      //: Label shown if a message doesn't have a sender.
                      //% "no sender"
                      qsTrId("whisperfish-sender-label-empty")
            recipient: sender.item
            isInGroup: isInGroup
            outbound: root.isOutbound
            maximumWidth: maxMessageWidth
            highlighted: down || root.highlighted
            width: delegateContentWidth
            backgroundGrow: contentPadding/2
            backgroundItem.radius: backgroundCornerRadius
            enabled: listView !== null && !listView.isSelecting
        }

        Item { width: 1; height: showSender ? senderNameLabel.backgroundGrow+Theme.paddingSmall : 0 }

        Loader {
            id: quoteItem
            active: showQuotedMessage
            asynchronous: true
            sourceComponent: Component {
                QuotedMessagePreview {
                    // id: quoteItem
                    visible: showQuotedMessage
                    width: delegateContentWidth
                    maximumWidth: maxMessageWidth
                    showCloseButton: false
                    showBackground: true
                    highlighted: down || root.highlighted
                    messageId: modelData.quotedMessageId ? modelData.quotedMessageId : -1
                    backgroundItem.roundedCorners: backgroundItem.bottomLeft |
                                                   backgroundItem.bottomRight |
                                                   (isOutbound ? backgroundItem.topRight :
                                                               backgroundItem.topLeft)
                    onClicked: {
                        if (listView.isSelecting) root.clicked(mouse)
                        else quoteClickedSignal(index, messageData)
                    }
                }
            }
        }

        Item { width: 1; height: quoteItem.shown ? Theme.paddingSmall : 0 }

        AttachmentsLoader {
            id: attachmentsLoader
            asynchronous: true
            enabled: hasAttachments && !isRemoteDeleted
            visible: enabled
            active: enabled
            width: delegateContentWidth
            cornersOutbound: isOutbound
            cornersQuoted: showQuotedMessage
            messageId: modelData.id
        }

        Item { width: 1; height: hasAttachments ? Theme.paddingSmall : 0 }

        Column {
            id: contentColumn
            width: delegateContentWidth
            height: (hasText || isEmpty) ? childrenRect.height : 0

            LinkedEmojiLabel {
                id: messageLabel
                visible: isEmpty || hasText
                plainText:  //: Placeholder note for a deleted message
                            //% "this message was deleted"
                            isRemoteDeleted ? qsTrId("whisperfish-message-deleted-note") :
                            //: Placeholder note if an empty message is encountered.
                            //% "this message is empty"
                            (isEmpty ? qsTrId("whisperfish-message-empty-note") :
                            ((needsRichText ? cssStyle : '') + (isExpanded ? _message : _message.substr(0, shortenThreshold) + (showExpand ? ' ...' : ''))))
                bypassLinking: true
                needsRichText: model.hasStrikeThrough || model.hasSpoilers
                hasSpoilers: model.hasSpoilers // Set to 'false' when text is clicked
                font.italic: isRemoteDeleted
                wrapMode: Text.Wrap
                anchors { left: parent.left; right: parent.right }
                horizontalAlignment: emojiOnly ? Text.AlignHCenter :
                                                 (isOutbound ? Text.AlignRight : Text.AlignLeft) // TODO make configurable
                color: isEmpty ?
                           (highlighted ? Theme.secondaryHighlightColor :
                                          (isOutbound ? Theme.secondaryHighlightColor :
                                                        Theme.secondaryColor)) :
                           (highlighted ? Theme.highlightColor :
                                          (isOutbound ? Theme.highlightColor :
                                                        Theme.primaryColor))
                linkColor: highlighted ? Theme.secondaryHighlightColor :
                                         Theme.secondaryColor
                enableCounts: true
                emojiOnlyThreshold: 5 // treat long messages as text
                font.pixelSize: emojiOnly ?
                                    (emojiCount <= 2 ? 1.5*Theme.fontSizeLarge :
                                                       1.0*Theme.fontSizeLarge) :
                                    Theme.fontSizeSmall // TODO make configurable
                defaultLinkActions: listView !== null && !listView.isSelecting
            }
        }

        Item {
            id: infoRow
            anchors {
                topMargin: Theme.paddingSmall * (hasReactions ? 2 : 1)
            }
            property real minContentWidth: emojiItem.width + Theme.paddingSmall + infoItem.width
            width: delegateContentWidth
            height: emojiItem.visible ? (emojiItem.height + Theme.paddingSmall) : infoItem.height

            EmojiItem {
                id: emojiItem
                reactions: reactions.status === Loader.Ready ? reactions.item.groupedReactions : ""
                anchors.top: parent.top
                anchors.left: isOutbound ? parent.left : undefined
                anchors.right: isOutbound ? undefined : parent.right
            }
            Loader {
                id: infoItem
                height: Theme.fontSizeExtraSmall
                asynchronous: true
                sourceComponent: Component { InfoItem { } }
                anchors.bottom: parent.bottom
                anchors.left: isOutbound ?  undefined : parent.left
                anchors.right: isOutbound ? parent.right : undefined
            }
        }
    }

    states: [
        State {
            name: "outbound"; when: isOutbound
            AnchorChanges { target: contentContainer; anchors.right: parent.right }
            AnchorChanges { target: replyArea; anchors.left: parent.left }
        },
        State {
            name: "inbound"; when: !isOutbound
            AnchorChanges { target: contentContainer; anchors.left: parent.left }
            AnchorChanges { target: replyArea; anchors.right: parent.right }
        }
    ]
}
