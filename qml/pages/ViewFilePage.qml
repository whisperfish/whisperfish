import QtQuick 2.5
import Sailfish.Silica 1.0
import "../components/attachment"

Page {
    id: page
    objectName: "viewImagePage"

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

        AttachmentItemFile {
            id: attachment
            anchors {
                horizontalCenter: parent.horizontalCenter
                top: header.bottom
                topMargin: Theme.itemSizeLarge
            }
            width: Math.min(page.width, page.height)
            height: Theme.itemSizeExtraLarge
            attach: page.attachment
            showThumbnail: true
            enableDefaultClickAction: false
            highlighted: false
        }
    }
}
