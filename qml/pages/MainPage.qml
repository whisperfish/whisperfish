import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Notifications 1.0
import QtQml.Models 2.2
import be.rubdos.whisperfish 1.0

import "../delegates"

Page {
    id: main

    objectName: "mainPage"

    property Sessions sessions
    readonly property real previewTextHeight: previewMetrics.height
                                              + previewMetrics.lineSpacing
                                              + previewMetrics.descent/2


    // Leaving the page drops the filter, so coming back
    // always shows the full list.
    onStatusChanged: {
        if (status === PageStatus.Inactive) {
            searchDebounce.stop()
            searchField.text = ""
            searchField.active = false
            visualSessionModel.searchText = ""
        }
    }

    Component.onCompleted: {
        var now = new Date()
        var alertTime = new Date()
        var ninety_days_ms = 1000*60*60*24*90
        var one_week_ms = 1000*60*60*24*7

        // Update detected (or last version not yet initialized)
        if (SettingsBridge.last_version != LongAppVersion) {
            alertTime.setTime(now.getTime() + ninety_days_ms)
            SettingsBridge.next_update_time = String(alertTime.getTime())
            SettingsBridge.last_version = String(LongAppVersion)
            return
        }

        alertTime.setTime(SettingsBridge.next_update_time)
        if (alertTime.getTime() < now.getTime()) {
            updateNotification.publish()

            alertTime.setTime(now.getTime() + one_week_ms)
            SettingsBridge.next_update_time = String(alertTime.getTime())
        }
    }

    SilicaListView {
        id: sessionView

        model: visualSessionModel
        anchors.fill: parent
        spacing: 0
        currentIndex: -1

        PullDownMenu {
            id: mainMenu

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
                    ClientWorker.reconnect()
                    //: Main page notification message when Whisperfish is trying to reconnect
                    //% "Reconnecting..."
                    notification.show(qsTrId("whisperfish-reconnecting"), "image://theme/icon-m-reload")
                }
            }

            MenuItem {
                //: Whisperfish main menu item: start a conversation by username/QR
                //% "New conversation"
                text: qsTrId("whisperfish-new-conversation-menu")
                visible: !SetupWorker.locked
                onClicked: pageStack.push(Qt.resolvedUrl("CreateConversationPage.qml"), { query: "" })
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

            MenuItem {
                //: Show search field menu item
                //% "Search"
                text: qsTrId("whisperfish-search-menu")
                visible: !SetupWorker.locked
                onClicked: pageStack.push(Qt.resolvedUrl("SearchPage.qml"),{ sessions:sessions })
            }

            MenuItem {
                visible: visualSessionModel.hasArchived || visualSessionModel.showArchived
                text: visualSessionModel.showArchived
                        //: Menu item for returning to "inbox" from archived sessions
                        //% "Return to conversations"
                      ? qsTrId("whisperfish-show-inbox-menu")
                        //: Menu item for showing archived conversations
                        //% "Show archived conversations"
                      : qsTrId("whisperfish-show-archived-menu")
                onClicked: visualSessionModel.showArchived = !visualSessionModel.showArchived
            }
        }

        header: Item {
            width: sessionView.width
            height: pageHeader.height
            visible: !SetupWorker.locked

            PageHeader {
                id: pageHeader

                opacity: searchField.active ? 0.0001 : 1.0
                Behavior on opacity { NumberAnimation {} }

                width: parent.width
                title: "Whisperfish"
                description: visualSessionModel.showArchived
                             //: Whisperfish subtitle for archived conversations
                             //% "Archived conversations"
                             ? qsTrId("whisperfish-subtitle-archived-conversations")
                             //: Whisperfish subtitle for active conversations aka. "inbox"
                             //% "Conversations"
                             : qsTrId("whisperfish-subtitle-active-conversations")
                visible: sessions.sessions.count > 0

                Icon {
                    id: statusIcon

                    anchors {
                        left: parent.extraContent.left
                        leftMargin: parent.leftMargin
                        verticalCenter: parent.verticalCenter
                    }
                    opacity: (!ClientWorker.queueEmpty || !ClientWorker.connected) ? 1.0 : 0.0
                    visible: opacity > 0.0
                    Behavior on opacity { NumberAnimation {} }

                    height: Theme.iconSizeMedium
                    width: Theme.iconSizeMedium

                    source: !ClientWorker.connected ? "image://theme/icon-s-blocked" : "image://theme/icon-m-sync"
                    color: Theme.highlightColor
                }

                Label {
                    anchors {
                        left: statusIcon.right
                        leftMargin: Theme.paddingMedium
                        verticalCenter: parent.verticalCenter
                    }
                    visible: statusIcon.visible
                    opacity: statusIcon.opacity
                    text: !ClientWorker.connected
                            ? //: Whisperfish connection status message
                              //% "Disconnected"
                              qsTrId("whisperfish-connection-status-disconnected")
                            : //: Whisperfish connection status message
                              //% "Loading messages"
                              qsTrId("whisperfish-connection-status-reconnecting")
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeSmall
                }
            }
        }

        VerticalScrollDecorator {}

        ViewPlaceholder {
            enabled: sessionView.count === 0
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
                } else if (visualSessionModel.searchText.length > 0) {
                    //: No conversations matched the search text
                    //% "No matching conversations"
                    qsTrId("whisperfish-no-search-results-hint-text")
                } else if (visualSessionModel.showArchived) {
                    //: Showing archived conversations, but the list is empty
                    //% "No archived conversations"
                    qsTrId("whisperfish-no-archived-messages-hint-text")
                } else {
                    // XXX Temporary text until
                    //     https://gitlab.com/whisperfish/whisperfish/-/issues/237
                    //: No messages found, hint on what to do
                    //% "Start a new conversation through a linked device, or let someone message you"
                    qsTrId("whisperfish-no-messages-hint-text-237")
                }
            }
        }

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
                        break
                    case "today":
                        //: Session section label for today
                        //% "Today"
                        qsTrId("whisperfish-session-section-today")
                        break
                    case "yesterday":
                        //: Session section label for yesterday
                        //% "Yesterday"
                        qsTrId("whisperfish-session-section-yesterday")
                        break
                    case "older":
                        //: Session section label for older
                        //% "Older"
                        qsTrId("whisperfish-session-section-older")
                        break
                    case "never":
                        //: Session section label for never (i.e. no messages, no timestamp)
                        //% "No messages"
                        qsTrId("whisperfish-session-section-never")
                        break
                    default:
                        var s = section.toString()
                        var k = s.substring(0, 1)
                        var i = parseInt(s.substring(1))
                        if (k == 'd') {
                            // 'dX' with X number requests a dayName
                            // two days to one week ago
                            return Qt.locale().standaloneDayName(i, Locale.LongFormat)
                        } else if (k == 'm') {
                            // 'mX' with X number requests a monthName
                            return Qt.locale().standaloneMonthName(i, Locale.LongFormat)
                        }
                        break
                    }
                }
            }
        }
    }

    Rectangle {
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        visible: visualSessionModel.sourceCount > 0
        y: (main.isPortrait ? Screen.height : Screen.width) - Theme.horizontalPageMargin - height
        width: Theme.iconSizeLarge
        height: Theme.iconSizeLarge
        radius: width / 5
        color: searchIcon.down
               ? Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
               : Theme.rgba(Theme.highlightDimmerColor, Theme.opacityOverlay)

        IconButton {
            id: searchIcon

            anchors.centerIn: parent
            height: Theme.iconSizeLarge
            width: Theme.iconSizeLarge

            icon.source: "image://theme/icon-m-search"
            icon.height: Theme.iconSizeMedium
            icon.width: Theme.iconSizeMedium

            onClicked: {
                sessionView.positionViewAtBeginning()
                searchField.active = true
                focusTimer.start()
            }
        }
    }

    // Lives outside the list, so the list's content geometry can't
    // displace it. Sticks to the top of the page, except that an
    // open pulley menu pushes it down like the rest of the content.
    Item {
        id: searchBar

        y: mainMenu.active
           ? Math.max(0, sessionView.originY - sessionView.contentY)
           : 0
        width: parent.width
        height: sessionView.headerItem ? sessionView.headerItem.height : 0
        visible: !SetupWorker.locked

        // Results scroll underneath
        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.highlightDimmerColor, Theme.opacityOverlay)
            opacity: searchField.active ? 1.0 : 0.0
            visible: opacity > 0.0
            Behavior on opacity { NumberAnimation {} }
        }

        SearchField {
            id: searchField

            anchors.bottom: parent.bottom
            canHide: true
            active: false
            focusOnClick: true

            //: Placeholder for the conversation search field
            //% "Search conversations"
            placeholderText: qsTrId("whisperfish-session-search-placeholder")
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText

            EnterKey.iconSource: "image://theme/icon-m-enter-close"
            EnterKey.onClicked: focus = false

            onTextChanged: searchDebounce.restart()
            onHideClicked: active = false

            Timer {
                id: focusTimer

                interval: 200
                onTriggered: searchField.forceActiveFocus()
            }
        }
    }

    /**
    * Rust gives us the sessions sorted by [isPinned, timestamp]. Which of
    * them are visible — archived vs. active, and whether they match the
    * search text — is decided here, so the Rust side doesn't have to care.
    *
    * Every row stays in the built-in "items" group for its whole life.
    * The "shown" group is what the view renders, and membership in it is
    * a pure function of accepts(). Items are never moved between groups,
    * so setting inShown cannot re-trigger the pass that set it.
    */
    DelegateModel {
        id: visualSessionModel

        property string searchText: ""
        property bool showArchived: false

        // id -> true for sessions matching searchText.
        // Empty object means "no search active, everything matches".
        property var matchedIds: ({})

        property bool hasArchived
        property int sourceCount: items.count

        model: sessions.sessions
        filterOnGroup: "shown"

        groups: [
            DelegateModelGroup {
                id: shownItems
                name: "shown"
                includeByDefault: false
            }
        ]

        delegate: SessionDelegate {
            id: sessionDelegate

            previewTextHeight: main.previewTextHeight
            onClicked: {
                console.log("Activating session: " + model.id)
                pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), { sessionId: model.id })
            }
            onRelocateItem: refilter.restart()
        }

        onSourceCountChanged: refilter.restart()
        onSearchTextChanged: rebuildMatches()
        onShowArchivedChanged: refilter.restart()

        Component.onCompleted: rebuildMatches()

        function digitsOf(s) {
            return String(s).replace(/[^0-9]/g, "")
        }

        // Assumes that q.length > 0
        function liteMatches(s, q) {
            if (s.name && String(s.name).toLowerCase().indexOf(q) >= 0)
                return true

            var aci = s.aci
            if (aci && String(aci).toLowerCase().indexOf(q) >= 0)
                return true

            var ext = (s.external_id !== undefined) ? s.external_id : s.externalId
            if (ext && String(ext).toLowerCase().indexOf(q) >= 0)
                return true

            if (String(s.e164 || "").indexOf(q) >= 0)
                return true

            return false
        }

        function rebuildMatches() {
            var q = searchText.toLowerCase()
            if (q.length === 0) {
                matchedIds = ({})
                refilter.restart()
                return
            }

            // TODO: cache the list until session changes
            var list = sessions.sessionNames
            var found = ({})

            for (var i = 0; i < list.length; i++) {
                var s = list[i]
                if (!s) continue
                if (liteMatches(s, q)) found[s.id] = true
            }

            matchedIds = found
            refilter.restart()
        }

        function accepts(m) {
            if (!m)
                return false
            if (!!m.isArchived !== showArchived)
                return false
            if (searchText.length === 0)
                return true
            return matchedIds[m.id] === true
        }

        function applyFilter() {
            var n = items.count
            var archived = false
            for (var i = 0; i < n; i++) {
                var it = items.get(i)
                var m = it.model
                if (m && m.isArchived)
                    archived = true
                it.inShown = accepts(m)
            }
            hasArchived = archived
        }
    }

    Notification {
        id: notification

        appName: "Whisperfish"
        expireTimeout: 3000

        function show(message, icn) {
            replacesId = 0
            previewSummary = ""
            previewBody = message
            icon = icn || ""
            publish()
        }
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

    FontMetrics {
        id: previewMetrics

        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSizeExtraSmall
    }

    Timer {
        id: searchDebounce

        interval: 300
        onTriggered: visualSessionModel.searchText = searchField.text.trim()
    }

    // Deferred so that a burst of model changes or keystrokes
    // collapses into one pass, and so that no pass ever runs
    // inside the change handler that requested it.
    Timer {
        id: refilter

        interval: 1
        onTriggered: visualSessionModel.applyFilter()
    }

    // Row contents can change without the row count changing:
    // archiving a session, renaming a contact or a group.
    Connections {
        target: sessions.sessions
        onDataChanged: refilter.restart()
        onModelReset: refilter.restart()
    }

    Connections {
        target: sessions
        onSessionNamesChanged: visualSessionModel.rebuildMatches()
    }
}
