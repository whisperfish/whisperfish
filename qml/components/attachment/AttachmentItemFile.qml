// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0

AttachmentItemBase {
    id: item
    property int recipientId: -1

    Recipient {
        id: recipient
        app: AppState
        recipientId: item.recipientId
    }

    onClicked: {
        if (_effectiveEnableClick) {
            pageStack.push(Qt.resolvedUrl('../../pages/ViewFilePage.qml'), {
                title: recipientId > -1 ? recipient.name : "",
                // Translated in QuotedMessagePreview.qml
                subtitle: qsTrId('whisperfish-quoted-message-preview-attachment'),
                'titleOverlay.subtitleItem.wrapMode': SettingsBridge.debug_mode ? Text.Wrap : Text.NoWrap,
                path: attach.data,
                attachmentId: attach.id,
                isViewOnce: false, // TODO: Implement attachment can only be viewed once
                attachment: attach,
            })
        }
    }

    Column {
        anchors {
            left: parent.left; right: parent.right
            verticalCenter: parent.verticalCenter
        }

        Label {
            highlighted: item.highlighted ? true : undefined
            text: _hasAttach ? attach.type : ''
            width: parent.width - Theme.paddingSmall
            elide: Text.ElideLeft
        }
        Label {
            text: _hasAttach
                ? (
                    attach.original_name != null && attach.original_name.length > 0
                    ? attach.original_name
                    : lastPartOfPath(attach.data)
                )
                : ''
            highlighted: item.highlighted ? true : undefined
            color: highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
            width: parent.width - Theme.paddingSmall
            font.pixelSize: Theme.fontSizeExtraSmall
            elide: Text.ElideLeft
        }
    }
}
