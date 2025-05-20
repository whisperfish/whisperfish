import QtQuick 2.5
import Sailfish.Silica 1.0
import "../components/attachment"

Page {
    id: page
    objectName: "viewAudioPage"

    // For new message notifications
    property int sessionId

    allowedOrientations: Orientation.All
    property alias title: header.title
    property alias subtitle: header.description
    property string path: ''
    property int attachmentId
    property bool isViewOnce
    property var attachment

    SilicaFlickable {
        id: flick
        anchors.fill: parent
        contentHeight: header.height + attachment.height

        PullDownMenu {
            MenuItem {
                enabled: attachmentId > 0 && !isViewOnce
                visible: enabled
                //: Copy the attachment file out of Whisperfish
                //% "Export file"
                text: qsTrId("whisperfish-export-file-menu")
                onClicked: {
                    MessageModel.exportAttachment(attachmentId)
                }
            }
        }

        PageHeader {
            id: header
        }

        AttachmentItemAudio {
            id: attachment
            anchors {
                horizontalCenter: parent.horizontalCenter
                top: header.bottom
                topMargin: Theme.itemSizeLarge
            }
            width: Math.min(page.width, page.height)
            height: Theme.itemSizeExtraLarge
            attach: page.attachment
            showThumbnail: false
            enableDefaultClickAction: false
            highlighted: false
        }

        Label {
            visible: attachment.isVoiceNote && attachment.transcription && attachment.transcription != ""
            wrapMode: _elideEnabled ? Text.WrapAnywhere : Text.Wrap
            font.pixelSize: Theme.fontSizeMedium
            text: attachment.transcription
        }
    }
}
