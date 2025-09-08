import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Notifications 1.0
import QtQml.Models 2.2
import be.rubdos.whisperfish 1.0

import "../delegates"

Page {
    id: main
    objectName: "mainPage"

    Notification {
        function show(message, icn) {
            replacesId = 0
            previewSummary = ""
            previewBody = message
            icon = icn || ""
            publish()
        }

        id: notification
        appName: "Whisperfish"
        expireTimeout: 3000
    }

    Sessions {
        id: sessions
        app: AppState
    }

    Notification {
        id: updateNotification
        appIcon: "harbour-whisperfish"
        appName: "Whisperfish"
        category: "harbour-whisperfish-update"

        //: Update notification title text
        //% "Please check for updates"
        previewSummary: qsTrId("whisperfish-update-reminder-summary")

        //: About whisperfish menu item
        //% "This Whisperfish release is more than 90 days old. Please check for an update in order to keep Whisperfish running smoothly."
        previewBody: qsTrId("whisperfish-update-reminder-body")
    }

    Component.onCompleted: {
        var now = new Date()
        var alertTime = new Date()
        var ninety_days_ms = 1000*60*60*24*90
        var one_week_ms = 1000*60*60*24*7

        // Update detected (or last version not yet initialized)
        if (SettingsBridge.last_version != LongAppVersion) {
            alertTime.setTime(now.getTime() + ninety_days_ms);
            SettingsBridge.next_update_time = String(alertTime.getTime())
            SettingsBridge.last_version = String(LongAppVersion)
            return;
        }

        alertTime.setTime(SettingsBridge.next_update_time)
        if (alertTime.getTime() < now.getTime()) {
            updateNotification.publish()

            alertTime.setTime(now.getTime() + one_week_ms);
            SettingsBridge.next_update_time = String(alertTime.getTime())
        }
    }

    SilicaFlickable {
        anchors.fill: parent

        VerticalScrollDecorator {}

        PullDownMenu {
            busy: !ClientWorker.queueEmpty || !ClientWorker.connected
            // NOTE Make sure the pulley menu doesn't have more
            // than four entries. 'New group' and 'New message'
            // can be merged; 'About' and 'Settings' maybe too.
            // This makes room for 'Search' and 'Bookmarks'.

            MenuItem {
                //: Whisperfish main menu item: restart websocket/reconnect
                //% "Reconnect"
                text: qsTrId("whisperfish-reconnect")
                onClicked: {
                    ClientWorker.reconnect();
                    //: Main page notification message when Whisperfish is trying to reconnect
                    //% "Reconnecting..."
                    notification.show(qsTrId("whisperfish-reconnecting"), "image://theme/icon-m-reload");
                }
            }
            MenuItem {
                text: "Call test"
                visible: SetupWorker.callingSupported && SettingsBridge.debug_mode
                onClicked: pageStack.push(Qt.resolvedUrl("RingingDialog.qml"))
            }
            MenuItem {
                //: About whisperfish menu item
                //% "About Whisperfish"
                text: qsTrId("whisperfish-about-menu")
                onClicked: pageStack.push(Qt.resolvedUrl("About.qml"))
            }
            MenuItem {
                //: Whisperfish settings menu item
                //% "Settings"
                text: qsTrId("whisperfish-settings-menu")
                visible: !SetupWorker.locked
                onClicked: pageStack.push(Qt.resolvedUrl("Settings.qml"))
            }
            /*
            MenuItem {
                // TODO implement in backend
                //: Show list of bookmarked messages, menu item
                //% "Bookmarks"
                text: qsTrId("whisperfish-bookmarks-menu")
                visible: sessions.hasBookmarks
                onClicked: pageStack.push(Qt.resolvedUrl("BookmarksPage.qml"))
            }
            */
            /*
            MenuItem {
                // TODO implement in backend (#13)
                //: Show search field menu item
                //% "Search"
                text: qsTrId("whisperfish-search-menu")
                visible: !SetupWorker.locked
                onClicked: pageStack.push(Qt.resolvedUrl("SearchPage.qml"))
            }
            */
            MenuItem {
                // TODO implement in backend
                visible: visualSessionModel.hasArchived || visualSessionModel.filterOnGroup === "archived"
                text: visualSessionModel.filterOnGroup === "active"
                        //: Menu item for showing archived conversations
                        //% "Show archived conversations"
                      ? qsTrId("whisperfish-show-archived-menu")
                        //: Menu item for returning to "inbox" from archived sessions
                        //% "Return to conversations"
                      : qsTrId("whisperfish-show-inbox-menu")
                onClicked: visualSessionModel.filterOnGroup = visualSessionModel.filterOnGroup === "archived"
                           ? "active"
                           : "archived"
            }
            /* TODO Disabled for now -- see #409
            MenuItem {
                // TODO merge "new group" and "new message" as "new conversation"
                //: Whisperfish new group menu item
                //% "New Group"
                text: qsTrId("whisperfish-new-group-menu")
                visible: !SetupWorker.locked
                onClicked: pageStack.push(Qt.resolvedUrl("NewGroup.qml"))
            }
            */
            MenuItem {
                //: Whisperfish new message menu item
                //% "New Message"
                text: qsTrId("whisperfish-new-message-menu")
                // visible: !SetupWorker.locked
                visible: false
                onClicked: pageStack.push(Qt.resolvedUrl("NewMessage.qml"))
            }
        }

        /**
         * Rust gives us the sessions sorted by [isPinned, timestamp] but in order to
         * split the messages into active and archived ones without rewriting/splitting
         * the SessionModel into two instances, DelegateModel can be used. This way
         * Rust side of things don't have to care if a message is archived or not.
         *
         * Basically "wrap" the SessionModel inside DelegateModel that handles this.
         */
        DelegateModel {
            id: visualSessionModel
            property bool hasArchived: false

            // Take the messages from "unsorted" group, and
            // push them either into "archived" or "artive"
            function sortToGroups() {
                var item
                var atLeastOneArchived = false
                while (unsortedItems.count > 0) {
                    item = unsortedItems.get(0)

                    if(item.model.isArchived) {
                        item.groups = "archived"
                        atLeastOneArchived = true
                    } else {
                        item.groups = "active"
                    }
                }
                visualSessionModel.hasArchived = atLeastOneArchived
            }

            // "Update was requested."
            // Find the session, remove it from its group
            // by setting it in the unsorted group.
            // This triggers the placement logic.
            function clearItemGroup(sessionId) {
                var item
                if(filterOnGroup === "active") {
                    for(var i = 0; i < activeItems.count; i++) {
                        item = activeItems.get(i)
                        if(item.model.id === sessionId) {
                            item.groups = "unsorted"
                            return
                        }
                    }
                } else {
                    for(var i = 0; i < archivedItems.count; i++) {
                        item = archivedItems.get(i)
                        if(item.model.id === sessionId) {
                            item.groups = "unsorted"
                            return
                        }
                    }
                }
            }

            // Don't show all items by default
            items.includeByDefault: false

            // Which group to show: active or archived sessions
            filterOnGroup: "active"

            groups: [
                // All added sessions (or removed and re-added)
                // are inserted into "unsorted" by default.
                DelegateModelGroup {
                    id: unsortedItems
                    name: "unsorted"

                    // When the group contents change,
                    // automatically sort them into groups.
                    includeByDefault: true
                    onChanged: {
                        visualSessionModel.sortToGroups()
                    }
                },
                DelegateModelGroup {
                    id: activeItems
                    name: "active"
                    includeByDefault: false
                },
                DelegateModelGroup {
                    id: archivedItems
                    name: "archived"
                    includeByDefault: false
                }
            ]
            model: sessions.sessions
            delegate: SessionDelegate {
                id: sessionDelegate
                onClicked: {
                    console.log("Activating session: " + model.id)
                    pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), { sessionId: model.id })
                }

                // On certain conditions, the session can request
                // the view to relocate itself.
                onRelocateItem: {
                    visualSessionModel.clearItemGroup(sessionId)
                }
            }
        }

        SilicaListView {
            id: sessionView
            model: visualSessionModel
            anchors.fill: parent
            spacing: 0
            header: PageHeader {
                title: "Whisperfish"
                description: visualSessionModel.filterOnGroup === "active"
                               //: Whisperfish subtitle for active conversations aka. "inbox"
                               //% "Conversations"
                             ? qsTrId("whisperfish-subtitle-active-conversations")
                               //: Whisperfish subtitle for archived conversations
                               //% "Archived conversations"
                             : qsTrId("whisperfish-subtitle-archived-conversations")
                visible: sessionView.count > 0

                Icon {
                    id: statusIcon
                    anchors {
                        left: parent.extraContent.left
                        // top: parent.extraContent.top
                        // margins: Theme.paddingLarge
                        leftMargin: parent.leftMargin
                        verticalCenter: parent.verticalCenter
                    }
                    visible: !ClientWorker.queueEmpty || !ClientWorker.connected
                    height: parent.height / 3
                    width: height

                    source: !ClientWorker.connected ? "image://theme/icon-s-blocked" : "image://theme/icon-m-sync"
                    color: Theme.highlightColor
                }

                Label {
                    anchors {
                        left: statusIcon.right
                        leftMargin: Theme.paddingMedium
                        verticalCenter: parent.verticalCenter
                    }
                    visible: !ClientWorker.queueEmpty || !ClientWorker.connected
                    text: !ClientWorker.connected
                            //: Whisperfish connection status message
                            //% "Disconnected"
                          ? qsTrId("whisperfish-connection-status-disconnected")
                            //: Whisperfish connection status message
                            //% "Synchronizing"
                          : qsTrId("whisperfish-connection-status-reconnecting")
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeSmall
                }
            }

            ViewPlaceholder {
                enabled: sessionView.count == 0
                // always show app name as placeholder, as the page
                // has not title which might be confusing
                text: "Whisperfish"
                hintText: {
                    if (!SetupWorker.registered) {
                        //: Whisperfish registration required message
                        //% "Registration required"
                        qsTrId("whisperfish-registration-required-message")
                    } else if (SetupWorker.locked) {
                        //: Whisperfish locked message
                        //% "Locked"
                        qsTrId("whisperfish-locked-message")
                    } else if(visualSessionModel.filterOnGroup === "active") {
                        // XXX Temporary branch for "active"
                        //     https://gitlab.com/whisperfish/whisperfish/-/issues/237
                        //: No messages found, hint on what to do
                        //% "Start a new conversation through a linked device, or let someone message you"
                        qsTrId("whisperfish-no-messages-hint-text-237")
                    } else if(visualSessionModel.filterOnGroup === "active") {
                        // XXX "Real" branch for "active"
                        //     https://gitlab.com/whisperfish/whisperfish/-/issues/237
                        //: No messages found, hint on what to do
                        //% "Pull down to start a new conversation."
                        qsTrId("whisperfish-no-messages-hint-text")
                    } else if(visualSessionModel.filterOnGroup === "archived") {
                        //: Showing archived conversations, but the list is empty
                        //% "No archived conversations"
                        qsTrId("whisperfish-no-archived-messages-hint-text")
                    } else {
                        ""
                    }
                }
            }

            height: contentHeight

            section {
                property: 'section'
                delegate: SectionHeader {
                    height: Theme.itemSizeExtraSmall
                    text: {
                        switch(section) {
                        case "pinned":
                            //: Session section label for pinned messages
                            //% "Pinned"
                            qsTrId("whisperfish-session-section-pinned")
                            break;
                        case "today":
                            //: Session section label for today
                            //% "Today"
                            qsTrId("whisperfish-session-section-today")
                            break;
                        case "yesterday":
                            //: Session section label for yesterday
                            //% "Yesterday"
                            qsTrId("whisperfish-session-section-yesterday")
                            break;
                        case "older":
                            //: Session section label for older
                            //% "Older"
                            qsTrId("whisperfish-session-section-older")
                            break;
                        case "never":
                            //: Session section label for never (i.e. no messages, no timestamp)
                            //% "No messages"
                            qsTrId("whisperfish-session-section-never")
                            break;
                        default:
                            // two days to one week ago
                            Qt.locale().dayName(parseInt(section), Locale.LongFormat)
                        }
                    }
                }
            }
        }
    }
}
