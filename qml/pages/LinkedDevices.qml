import QtQuick 2.2
import Sailfish.Silica 1.0

Page {
    id: linkedDevices
    objectName: "linkedDevicesPage"

    SilicaListView {
        id: listView
        anchors.fill: parent
        spacing: Theme.paddingMedium
        model: DeviceModel

        PullDownMenu {
            MenuItem {
                //: Menu option to add new linked device
                //% "Add"
                text: qsTrId("whisperfish-add-linked-device")
                onClicked: {
                    var d = pageStack.push(Qt.resolvedUrl("AddDevice.qml"))
                    d.addDevice.connect(function(tsurl) {
                        console.log("Add device: "+tsurl)
                        // TODO: handle errors
                        ClientWorker.link_device(tsurl)
                    })
                }
            }
            MenuItem {
                //: Menu option to refresh linked devices
                //% "Refresh"
                text: qsTrId("whisperfish-refresh-linked-devices")
                onClicked: {
                    ClientWorker.reload_linked_devices()
                }
            }
        }
        header: PageHeader {
            //: Title for Linked Devices page
            //% "Linked Devices"
            title: qsTrId("whisperfish-linked-devices")
        }
        Loader {
            id: addDeviceLoader
            visible: false
            source: "AddDevice.qml"
        }
        delegate: ListItem {
            contentHeight: created.y + created.height + lastSeen.height + Theme.paddingMedium
            id: delegate
            menu: deviceContextMenu

            function remove(contentItem) {
                //: Unlinking remorse info message for unlinking secondary devices (past tense)
                //% "Unlinked"
                contentItem.remorseAction(qsTrId("whisperfish-device-unlink-message"),
                    function() {
                        console.log("Unlink device: ", model)
                        ClientWorker.unlink_device(model.id)
                        ClientWorker.reload_linked_devices()
                    })
            }

            Label {
                id: name
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeMedium
                text: if (model.name) {
                    model.name
                } else if (model.id == 1) {
                    //: Linked device title for current Whisperfish
                    //% "Current device (Whisperfish, %1)"
                    qsTrId("whisperfish-current-device-name").arg(model.id)
                } else {
                    //: Linked device name
                    //% "Device %1"
                    qsTrId("whisperfish-device-name").arg(model.id)
                }
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                }
            }
            Label {
                function createdTime() {
                    var linkDate = Format.formatDate(model.created, Formatter.Timepoint)
                    //: Linked device date
                    //% "Linked: %1"
                    return qsTrId("whisperfish-device-link-date").arg(linkDate)
                }
                id: created
                text: createdTime()
                font.pixelSize: Theme.fontSizeExtraSmall
                anchors {
                    top: name.bottom
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                }
            }
            Label {
                id: lastSeen
                function lastSeenTime() {
                    var diff = (new Date()).valueOf() - model.lastSeen.valueOf()
                    console.log(model.lastSeen.toString())

                    var ls = ""
                    if(diff < 86400000) {
                        // Reused from MainPage.qml
                        ls = qsTrId("whisperfish-session-section-today")
                    } else if (diff < 172800000) {
                        // Reused from MainPage.qml
                        ls = qsTrId("whisperfish-session-section-yesterday")
                    } else {
                        ls = Format.formatDate(model.lastSeen, Formatter.DurationElapsed)
                    }
                    //: Linked device last active date
                    //% "Last active: %1"
                    return qsTrId("whisperfish-device-last-active").arg(ls)
                }
                text: lastSeenTime()
                font.pixelSize: Theme.fontSizeExtraSmall
                font.italic: true
                anchors {
                    top: created.bottom
                    topMargin: Theme.paddingSmall
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                }
            }
            Component {
                id: deviceContextMenu
                ContextMenu {
                    id: menu
                    width: parent ? parent.width : Screen.width
                    MenuItem {
                        //: Device unlink menu option
                        //% "Unlink"
                        text: qsTrId("whisperfish-device-unlink")
                        onClicked: remove(menu.parent)
                        enabled: model.id != 1
                    }
                }
            }
        }
        ViewPlaceholder {
            enabled: listView.count == 0
            //: Placeholder when no linked device yet
            //% "No linked device"
            text: qsTrId("whisperfish-device-placeholder")
            //: Placeholder hint when no linked device yet
            //% "Pull down to link Whisperfish to another device"
            hintText: qsTrId("whisperfish-device-placeholder-hint")
        }
    }
}
