// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"
import "../components/message"

ListItem {
    id: root
    width: parent.width
    contentHeight: contentContainer.height
    highlighted: down || menuOpen || replyArea.pressed || isSelected
    _backgroundColor: "transparent"

    // REQUIRED PROPERTIES
    property QtObject modelData
    // TODO the quoted message should be a notifyable object from a model
    // TODO we need a way to get a valid index from a message id
    //      (we must rely on the message's id instead of its index, as the latter may change)
    // TODO 'attachments' is expected as a list of objects: [{data: path, type: mimetype}, ...]
    property var quotedMessageData: null // required properties: message, source, outgoing, attachments, AND id, index
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
    // TODO Attachments with mimetype text/x-signal-plain have to be
    // treated as extra long messages.
    readonly property string _message: hasData && modelData.message ? modelData.message.trim() : ''
    readonly property string _source: hasData && modelData.source ? modelData.source.trim() : ''
    readonly property var _attachments: hasData && modelData.attachment && modelData.mimeType ?
                                            [{ data: modelData.attachment, type: modelData.mimeType }] : []
    // TODO implement multiple attachments in the model (separate thumbs/details; include mime type)
    // TODO implement shared locations (show a map etc.; is probably not an attachment)

    // SETTINGS PROPERTIES
    property var contactName: contact !== null ? contact.displayLabel : _source
    property var contact: isOutbound || !hasSource ? null : resolvePeopleModel.personByPhoneNumber(_source)

    // All children are placed inside a bubble, positioned left or right for
    // incoming/outbound messages. The bubble extends slightly over the contents.
    default property alias delegateContents: contentColumn.data
    property real contentPadding: 2*Theme.paddingMedium
    property real delegateContentWidth: isExpanded ? _expandedWidth : _unexpandedWidth
    property real _expandedWidth: width - 2*Theme.horizontalPageMargin // page width
    property real _unexpandedWidth: hasAttachments ?
                                        maxMessageWidth :
                                        Math.min(Math.max(metrics.width+messageLabel.emojiCount *
                                                          messageLabel.font.pixelSize, minMessageWidth) +
                                                 Theme.paddingMedium, maxMessageWidth)
    property real maxMessageWidth: parent.width - 6*Theme.horizontalPageMargin
    property real minMessageWidth: Math.max(showSender ? senderNameLabel.implicitWidth : 0,
                                            showQuotedMessage ? quoteItem.implicitWidth : 0,
                                            showExpand ? maxMessageWidth : infoRow.minContentWidth)
    property int shortenThreshold: 600 // in characters
    property int extraPageTreshold: 1500 // in characters
    property bool expandExtraPage: showExpand && (_message.length > extraPageTreshold)
    property real backgroundCornerRadius: Theme.paddingLarge

    property bool showSender: (isInGroup && !isOutbound) || !hasSource
    property bool showQuotedMessage: hasQuotedMessage
    property bool showExpand: !isEmpty && _message.length > shortenThreshold

    readonly property bool hasData: modelData !== null && typeof modelData !== 'undefined'
    readonly property bool hasQuotedMessage: quotedMessageData !== null
    readonly property bool hasAttachments: hasData && (modelData.hasAttachment === true)
    readonly property bool hasText: hasData && _message !== ''
    readonly property bool hasSource: hasData && _source !== ''
    readonly property bool isOutbound: hasData && (modelData.outgoing === true)
    readonly property bool isInGroup: MessageModel.group
    readonly property bool isEmpty: !hasText && !hasAttachments
    property bool isExpanded: false
    property bool isSelected: listView.selectedMessages[modelData.id] !== undefined

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
            pageStack.push("../pages/ExpandedMessagePage.qml", { 'modelData': modelData })
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

    Loader {
        id: background
        anchors { fill: contentContainer; margins: contentPadding/3 }
        asynchronous: true
        sourceComponent: Component {
            RoundedRect {
                radius: backgroundCornerRadius
                roundedCorners: isOutbound ? bottomLeft | topRight : bottomRight | topLeft
                color: (down || replyArea.pressed) ? Theme.highlightBackgroundColor : Theme.secondaryColor
                opacity: (down || replyArea.pressed) ?
                             (isOutbound ? 0.7*Theme.opacityFaint : 1.0*Theme.opacityFaint) :
                             (isOutbound ? 0.4*Theme.opacityFaint : 0.8*Theme.opacityFaint)
            }
        }
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
            text: hasSource ?
                      contactName :
                      //: Label shown if a message doesn't have a sender.
                      //% "no sender"
                      qsTrId("whisperfish-sender-label-empty")
            source: (isOutbound || !hasSource) ? '' : _source
            outbound: root.isOutbound
            maximumWidth: maxMessageWidth
            highlighted: down || root.highlighted
            width: delegateContentWidth
            backgroundGrow: contentPadding/2
            backgroundItem.radius: backgroundCornerRadius
            enabled: !listView.isSelecting
        }

        Item { width: 1; height: showSender ? senderNameLabel.backgroundGrow+Theme.paddingSmall : 0 }

        QuotedMessagePreview {
            id: quoteItem
            visible: showQuotedMessage
            width: delegateContentWidth
            maximumWidth: maxMessageWidth
            showCloseButton: false
            showBackground: true
            highlighted: down || root.highlighted
            messageData: quotedMessageData
            backgroundItem.roundedCorners: backgroundItem.bottomLeft |
                                           backgroundItem.bottomRight |
                                           (isOutbound ? backgroundItem.topRight :
                                                       backgroundItem.topLeft)
            onClicked: {
                if (listView.isSelecting) root.clicked(mouse)
                else quoteClickedSignal(index, messageData)
            }
        }

        Item { width: 1; height: quoteItem.shown ? Theme.paddingSmall : 0 }

        AttachmentsLoader {
            asynchronous: true
            enabled: hasAttachments
            width: delegateContentWidth
            cornersOutbound: isOutbound
            cornersQuoted: showQuotedMessage
        }

        Item { width: 1; height: hasAttachments ? Theme.paddingSmall : 0 }

        Column {
            id: contentColumn
            width: delegateContentWidth
            height: (hasText || isEmpty) ? childrenRect.height : 0

            LinkedEmojiLabel {
                id: messageLabel
                visible: isEmpty || hasText
                plainText: isEmpty ?
                               //: Placeholder note if an empty message is encountered.
                               //% "this message is empty"
                               qsTrId("whisperfish-message-empty-note") :
                               (isExpanded ? _message : _message.substr(0, shortenThreshold) + (showExpand ? ' ...' : ''))
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
                defaultLinkActions: !listView.isSelecting
            }
        }

        Item { width: 1; height: Theme.paddingSmall }

        Loader {
            id: infoRow
            property real minContentWidth: item ? item.minContentWidth : 0
            width: delegateContentWidth
            height: Theme.fontSizeExtraSmall
            asynchronous: true
            sourceComponent: Component { InfoRow { } }
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
