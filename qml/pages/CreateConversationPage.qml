import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../delegates"
import "../components"

Page {
    id: root
    objectName: "createConversationPage"

    property alias sessionId: createConversation.sessionId
    property alias serviceId: createConversation.serviceId
    property alias name: createConversation.name
    property alias query: createConversation.query

    // True when the page was pushed without a serviceId — i.e. the user is
    // entering a username/link manually or scanning a QR. The direct path
    // (group-member dropdown, which pushes {serviceId, name}) flips this off
    // and shows only the resolving spinner, preserving the prior behaviour.
    property bool inputMode: serviceId.length === 0

    // Guards the double-push: `attemptTransition` fires from both
    // `onSessionIdChanged` and the pageStack `onBusyChanged` (the replace
    // animation toggles `busy`, which re-triggers the transition). Without a
    // guard, resolving a username whose conversation already exists pushes
    // the ConversationPage twice. Latched once on the first successful
    // replace.
    property bool _transitioned: false

    // Live client-side validation.
    function _isUsername(s) {
        return typeof s === "string" && /^[A-Za-z_][A-Za-z0-9_]{2,31}\.[0-9]{2,9}$/.test(s)
    }
    function _isLink(s) {
        return typeof s === "string" && ( /signal\.me\/?#eu\//.test(s) || /^[A-Za-z0-9_-]{20,}$/.test(s) )
    }
    function _isValidQuery(s) {
        return _isUsername(s) || _isLink(s)
    }
    function _validationHint(s) {
        if (!s || s.length === 0) return ""
        if (_isUsername(s)) return ""
        if (_isLink(s)) return ""
        if (s.indexOf("@") === 0) {
            //: Live validation hint for a leading @
            //% "Usernames don't start with @."
            return qsTrId("whisperfish-username-leading-at-hint")
        }
        //: Live validation hint when the entered text is neither a username nor a link
        //% "Enter a username (e.g. johndoe.99) or a signal.me link."
        return qsTrId("whisperfish-username-format-hint")
    }

    function attemptTransition() {
        if (_transitioned) return
        if (sessionId != -1) {
            _transitioned = true
            if (pageStack.busy) {
                pageStack.completeAnimation();
            } else {
                pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), { sessionId: sessionId });
            }
        }
    }

    function launchQrScanner() {
        var page = pageStack.push(Qt.resolvedUrl("UsernameQrScannerPage.qml"))
        page.resultFound.connect(function (link) {
            // Drives set_query → ResolveUsername on the resolver subactor.
            createConversation.query = link
        })
    }

    function errorDisplayText() {
        // Maps the model's error id to a human label; unknown/empty ids show empty.
        var e = createConversation.error || ""
        switch (e) {
        case "whisperfish-username-not-found":
            //: Username lookup completed but no account matched
            //% "%1 is not a Signal user. Make sure you've entered the complete username."
            return qsTrId("whisperfish-username-not-found-text")
        case "whisperfish-username-resolver-unavailable":
            //: Username resolver actor not available (boot-window race)
            //% "Not ready yet. Please try again in a moment."
            return qsTrId("whisperfish-username-resolver-unavailable-text")
        case "whisperfish-username-lookup-failed":
            //: Username lookup failed for a generic reason (network / malformed link)
            //% "Couldn't look up this username or link."
            return qsTrId("whisperfish-username-lookup-failed-text")
        default:
            return e
        }
    }

    CreateConversation {
        id: createConversation
        app: AppState
        // properties set through aliases

        onSessionIdChanged: {
            attemptTransition();
        }
    }

    Connections {
        target: pageStack
        onBusyChanged: {
            attemptTransition();
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        // PullDownMenu must be a child of the SilicaFlickable (the Sailfish
        // idiom, per MainPage.qml / Settings.qml); placing it as a page-level
        // sibling makes the pulley attach to the page's implicit/null
        // flickable, producing the "Cannot read property 'contentX' of null"
        // framework warnings during show/teardown races.
        PullDownMenu {
            // Only the QR scan entry is page-local input; the username field is
            // the primary entry and is always visible in input mode.
            MenuItem {
                //: Pull-down menu item to scan a username-link QR code
                //% "Scan QR code"
                text: qsTrId("whisperfish-username-scan-qr-menu")
                visible: root.inputMode && !createConversation.busy
                onClicked: root.launchQrScanner()
            }
            MenuItem {
                //: Pull-down menu item to clear the current query and retry
                //% "Clear"
                text: qsTrId("whisperfish-username-clear-menu")
                visible: root.inputMode && ((createConversation.query || "").length > 0 || (createConversation.error || "").length > 0)
                onClicked: {
                    queryField.text = ""
                    createConversation.query = ""
                }
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: createConversation.hasName
                    ? createConversation.name
                    //: Page header title when a new conversation is being created
                    //% "New conversation"
                    : qsTrId("whisperfish-creating-conversation-title")
                description: createConversation.hasName
                    //: Repeat of the title in the header description when a name is known
                    //% "Creating conversation"
                    ? qsTrId("whisperfish-creating-conversation-title")
                    : ""
            }

            Label {
                visible: root.inputMode && (createConversation.query || "").length === 0 && (createConversation.error || "").length === 0
                width: parent.width - 2 * Theme.horizontalPageMargin
                x: Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                //: Instructional text on the empty create-conversation page
                //% "Enter a Signal username (like johndoe.99) or paste a signal.me link to start a conversation. You can also scan a QR code from the pull-down menu."
                text: qsTrId("whisperfish-username-instructions")
            }

            TextField {
                id: queryField
                visible: root.inputMode
                width: parent.width
                //: Placeholder for the username/link entry field
                //% "Username or signal.me link"
                placeholderText: qsTrId("whisperfish-username-query-placeholder")
                //: Label for the username/link entry field
                //% "Username"
                label: qsTrId("whisperfish-username-query-label")
                text: createConversation.query
                inputMethodHints: Qt.ImhNoPredictiveText
                enabled: !createConversation.busy
                // Live validation: only allow submit when the text plausibly
                // matches a username or link. The detailed hint explains why.
                EnterKey.enabled: root._isValidQuery(queryField.text)
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: {
                    if (root._isValidQuery(queryField.text)) {
                        createConversation.query = queryField.text
                    }
                }
            }

            Label {
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.horizontalCenter: parent.horizontalCenter
                horizontalAlignment: Qt.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                // Live format hint; hidden once the text matches a known shape
                // or once a lookup is underway (busy) or has produced an error.
                visible: root.inputMode
                    && !createConversation.busy
                    && (createConversation.error || "").length === 0
                    && queryField.text.length > 0
                    && !root._isValidQuery(queryField.text)
                text: root._validationHint(queryField.text)
            }

            BusyIndicator {
                size: BusyIndicatorSize.Large
                anchors.horizontalCenter: parent.horizontalCenter
                running: createConversation.busy
                visible: root.inputMode
            }

            Label {
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.horizontalCenter: parent.horizontalCenter
                horizontalAlignment: Qt.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.errorColor
                font.pixelSize: Theme.fontSizeSmall
                visible: root.inputMode && (createConversation.error || "").length > 0
                text: root.errorDisplayText()
            }
        }
    }

    // Direct-mode spinner centred on the page (preserves the pre-username
    // behaviour where the group-member path resolves a known serviceId).
    BusyIndicator {
        size: BusyIndicatorSize.Large
        anchors.centerIn: parent
        running: !root.inputMode && !createConversation.ready
    }
}
