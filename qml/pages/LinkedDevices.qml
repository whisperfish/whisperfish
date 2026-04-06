import QtQuick 2.2
import Sailfish.Silica 1.0

Page {
    id: linkedDevices

    objectName: "linkedDevicesPage"

    property bool is_primary_device: SettingsBridge.isPrimaryDevice()

    SilicaListView {
        id: listView

        anchors.fill: parent
        spacing: Theme.paddingMedium
        model: DeviceModel

        header: PageHeader {
            //: Title for Linked Devices page
            //% "Linked Devices"
            title: qsTrId("whisperfish-linked-devices")
        }

        delegate: ListItem {
            id: delegate

            contentHeight: created.y + created.height + lastSeen.height + Theme.paddingMedium
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

                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                }
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeMedium

                // TODO: handle the current device differently?
                text: if (model.name) {
                    model.name
                } else if (model.id == 1) {
                    //: The nameless primary device in linked devices list
                    //% "Primary device"
                    qsTrId("whisperfish-primary-device-name")
                } else {
                    //: A nameless secondary device in linked devices list
                    //% "Device %1"
                    qsTrId("whisperfish-secondary-device-name").arg(model.id)
                }
            }

            Label {
                id: created

                anchors {
                    top: name.bottom
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                }
                text: createdTime()
                font.pixelSize: Theme.fontSizeExtraSmall

                function createdTime() {
                    var linkDate = Format.formatDate(model.created, Formatter.Timepoint)
                    //: Linked device date
                    //% "Linked: %1"
                    return qsTrId("whisperfish-device-link-date").arg(linkDate)
                }
            }

            Label {
                id: lastSeen

                anchors {
                    top: created.bottom
                    topMargin: Theme.paddingSmall
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                }
                text: lastSeenTime()
                font.pixelSize: Theme.fontSizeExtraSmall
                font.italic: true

                function lastSeenTime() {
                    var diff = (new Date()).valueOf() - model.lastSeen.valueOf()

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
            }
            Component {
                id: deviceContextMenu

                ContextMenu {
                    id: menu

                    enabled: is_primary_device
                    visible: enabled
                    width: parent ? parent.width : Screen.width

                    MenuItem {
                        enabled: model.id > 1
                        visible: enabled
                        //: Rename the linked or primary device menu option
                        //% "Rename"
                        text: qsTrId("whisperfish-device-rename")

                        onClicked: pageStack.push(Qt.resolvedUrl("RenameDevicePage.qml"), {
                            device_id: model.id,
                            device_name: model.name
                        })
                    }

                    MenuItem {
                        //: Device unlink menu option
                        //% "Unlink"
                        text: qsTrId("whisperfish-device-unlink")
                        enabled: model.id > 1
                        visible: enabled

                        onClicked: remove(menu.parent)
                    }
                }
            }
        }

        PullDownMenu {
            MenuItem {
                enabled: is_primary_device
                visible: enabled
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

        Loader {
            id: addDeviceLoader

            visible: false
            source: is_primary_device ? "AddDevice.qml" : ""
            asynchronous: true
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
