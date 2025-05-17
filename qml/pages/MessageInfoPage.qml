import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../components"
import "../delegates"

Page {
    id: root
    objectName: "messageInfoPage"

    property var message

    // For new message notifications
    property int sessionId
    property bool isInGroup

    // Proxy some more used properties
    readonly property bool outgoing: message.outgoing
    readonly property var debugMode: SettingsBridge.debug_mode
    readonly property var deliveryReceipts: message.deliveredReceipts
    readonly property var readReceipts: message.readReceipts
    readonly property var viewedReceipts: message.viewedReceipts

    Reactions {
        id: reactions
        app: AppState
        messageId: message.id
    }

    SilicaFlickable {
        id: silicaFlickable
        anchors.fill: parent

        contentHeight: contentColumn.height + Theme.paddingLarge

        VerticalScrollDecorator {
            flickable: silicaFlickable
        }

        Column {
            id: contentColumn
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            spacing: Theme.paddingMedium

            PageHeader {
                id: pageHeader
                //: Page title for message info/details page
                //% "Message Info"
                title: qsTrId("whisperfish-message-info-title")
            }

            Item {
                // TODO: Disable touches properly.
                // 'enabled: false' messes up visuals
                id: messageItem
                property bool atSectionBoundary: false
                property bool isServiceMessage: false

                height: loader.y + loader.height
                width: parent.width

                Loader {
                    id: loader
                    width: parent.width
                    sourceComponent: defaultMessageDelegate
                }

                Component {
                    id: defaultMessageDelegate
                    MessageDelegate {
                        id: messageDelegate
                        modelData: message
                        isInGroup: isInGroup
                        //menu: messageContextMenu
                        // set explicitly because attached properties are not available
                        // inside the loaded component
                        showSender: true
                        // No menus here!
                        openMenuOnPressAndHold: false
                    }
                }
            }

            DetailItem {
                visible: debugMode
                //: Label for id of the message (in database)
                //% "Message ID"
                label: qsTrId("whisperfish-message-message-id")
                value: message.id
            }
            DetailItem {
                visible: debugMode
                //: Label for session id of the message (in database)
                //% "Session ID"
                label: qsTrId("whisperfish-message-session-id")
                value: sessionId
            }

            // TIMESTAMP

            DetailItem {
                //: Label for the timestamp of the message
                //% "Timestamp"
                label: qsTrId("whisperfish-message-timestamp")
                value: message.timestamp
            }

            // REACTIONS

            SectionHeader {
                visible: reactions.count
                //: Reactions section header
                //% "Reactions"
                text: qsTrId("whisperfish-message-info-reactions")
            }
            Repeater {
                model: reactions.reactions
                DetailItem {
                    label: model.name
                    value: model.reaction
                }
            }

            // DELIVERY RECEIPTS

            SectionHeader {
                visible: deliveryReceipts.length > 0
                //: Delivered receipts section header
                //% "Delivery receipts"
                text: qsTrId("whisperfish-message-info-delivery-receipts")
            }
            Repeater {
                model: deliveryReceipts
                DetailItem {
                    label: modelData.recipient
                    value: modelData.timestamp
                }
            }

            // READ RECEIPTS

            SectionHeader {
                visible: readReceipts.length > 0 > 0
                //: Read receipts section header
                //% "Read receipts"
                text: qsTrId("whisperfish-message-info-read-receipts")
            }
            Repeater {
                model: readReceipts
                DetailItem {
                    label: modelData.recipient
                    value: modelData.timestamp
                }
            }

            // VIEWED RECEIPTS

            SectionHeader {
                visible: viewedReceipts.length > 0
                //: Viewed receipts section header
                //% "Viewed receipts"
                text: qsTrId("whisperfish-message-info-viewed-receipts")
            }
            Repeater {
                model: viewedReceipts
                DetailItem {
                    label: modelData.recipient
                    value: modelData.timestamp
                }
            }
        }
    }
}
