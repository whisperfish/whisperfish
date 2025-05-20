import QtQuick 2.2
import Sailfish.Silica 1.0
import "../components"
import "../delegates"

Page {
    id: root
    objectName: "expandedMessagePage"

    property QtObject modelData
    property bool isInGroup
    property string messageText: ""
    property string contactName: ""

    property var detailAttachments: modelData.detailAttachments
    property int detailAttachmentCount: detailAttachments !== undefined ? detailAttachments.count : 0

    Component.onCompleted: {
        var textFound = false
        var attachment = null
        for (var i = 0; i < detailAttachmentCount; i++) {
            attachment = detailAttachments.get(i)
            if (attachment.type == "text/x-signal-plain") {
                textFound = true
                break
            }
        }
        if(textFound) {
            var xhr = new XMLHttpRequest
            xhr.open("GET", attachment.data)
            xhr.onreadystatechange = function() {
                if (xhr.readyState == XMLHttpRequest.DONE) {
                    root.messageText = xhr.responseText
                }
            }
            xhr.send()
        } else {
            root.messageText = modelData.message.trim()
        }
    }

    SilicaFlickable {
        id: flick
        anchors.fill: parent
        contentHeight: column.height + Theme.horizontalPageMargin

        VerticalScrollDecorator { flickable: flick }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                //: Page title for a very long message shown on a page of its own
                //% "Full message"
                title: qsTrId("whisperfish-expanded-message-page-header")
                description: (delegate.isOutbound ?
                                  //: Page description for a very long message shown on a page of its own
                                  //% "to %1"
                                  qsTrId("whisperfish-expanded-message-info-outbound") :
                                  //: Page description for a very long message shown on a page of its own
                                  //% "from %1"
                                  qsTrId("whisperfish-expanded-message-info-inbound")).
                              arg(contactName)
            }

            MessageDelegate {
                id: delegate
                modelData: root.modelData
                isInGroup: session.isGroup
                enabled: false
                delegateContentWidth: root.width - 4*Theme.horizontalPageMargin
                isExpanded: true
                showExpand: false
                fullMessageText: messageText
                extraPageTreshold: _message.length
            }
        }
    }
}
