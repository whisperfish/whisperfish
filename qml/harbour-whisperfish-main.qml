import QtQuick 2.2
import Sailfish.Silica 1.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0
import org.nemomobile.contacts 1.0 as Contacts
import "pages"

// Note: This is the main QML file for Whisperfish.
// The signalcaptcha helper uses harbour-whisperfish.qml by design.

ApplicationWindow
{
    id: mainWindow
    cover: Qt.resolvedUrl("cover/CoverPage.qml")
    initialPage: Component { LandingPage { } }
    allowedOrientations: Orientation.All
    _defaultPageOrientations: Orientation.All
    _defaultLabelFormat: Text.PlainText

    property var notificationMap: ({})
    property var notificationQueue: ([])
    property var _mainPage: null

    // setting this to "true" will block global navigation
    // methods (showMainPage() etc.)
    property bool fatalOccurred: false

    property alias contactsReady: resolvePeopleModel.populated

    property string shareClientId: ""
    property string proofCaptchaToken: ''

    Contacts.PeopleModel {
        id: resolvePeopleModel

        // Specify the PhoneNumberRequired flag to ensure that all phone number
        // data will be loaded before the model emits populated.
        // This ensures that we resolve numbers to contacts appropriately, in
        // the case where we attempt to message a newly-created contact via
        // the action shortcut icon in the contact card.
        requiredProperty: PeopleModel.PhoneNumberRequired

        property var person: Component { Contacts.Person { } }

        function createContact(e164, first, last) {
            return person.createObject(null, {
                'firstName': first ? first : '',
                'lastName': last ? last : '',
                'phoneDetails': [{
                    'type': 11, // phone number type
                    'number': e164,
                    'index': -1
                }]
            })
        }
    }

    Component {
        id: messageNotification
        Notification {
            property int messageId
            appIcon: "harbour-whisperfish"
            appName: "Whisperfish"
            category: "harbour-whisperfish-message"
            Component.onDestruction: close()
        }
    }

    Component {
        id: callNotification
        Notification {
            property int sessionId
            appIcon: "harbour-whisperfish"
            appName: "Whisperfish"
            category: "harbour-whisperfish-call"
            Component.onDestruction: close()
        }
    }

    Notification {
        id: quietMessageNotification
        property bool isSupported: false

        Component.onCompleted: {
            if(quietMessageNotification.sound !== undefined) {
                quietMessageNotification.sound = "/usr/share/sounds/jolla-ambient/stereo/jolla-related-message.wav"
                quietMessageNotification.isSupported = true
            }
        }
    }

    /// Helper function to mimic "??" operator for easier assignment of maybe undefined/null JS strings
    function valueOrEmptyString(value) {
        if (value != null)
            return value
        else
            return ""
    }

    function getGroupAvatar(groupId) {
        if(!groupId) {
            return ""
        }

        if (SettingsBridge.avatarExists(groupId)) {
            return "file://" + SettingsBridge.avatar_dir + "/" + groupId
        } else {
            return ""
        }

    }

    // Return peer contacts avatar or Signal profile avatar based on
    // user selected preference. Do not use for groups (there's no choice).
    function getRecipientAvatar(e164, uuid, extId) {
        var contact = null
        // In Sailfish OS, extId is a number
        if (extId != null) {
            extId = parseInt(extId)
            contact = contactsReady ? resolvePeopleModel.personById(extId) : null
        }
        if (contact == null && e164 != null && e164[0] === '+') {
            // Only try to search for contact name if contact is a phone number
            contact = contactsReady ? resolvePeopleModel.personByPhoneNumber(e164, true) : null
        }

        var contact_avatar = (contact && contact.avatarPath) ? contact.avatarPath.toString() : ''
        var contact_avatar_ok = contact_avatar !== '' && contact_avatar.indexOf('image://theme/') !== 0

        var signal_avatar = uuid !== undefined ? "file://" + SettingsBridge.avatar_dir + "/" + uuid : ''
        var signal_avatar_ok = uuid !== undefined ? SettingsBridge.avatarExists(uuid) : false

        if(signal_avatar_ok && contact_avatar_ok) {
            return SettingsBridge.prefer_device_contacts ? contact_avatar : signal_avatar
        } else if (signal_avatar_ok) {
            return signal_avatar
        } else if (contact_avatar_ok) {
            return contact_avatar
        }
        return ""
    }

    // Return either given peer name or device contacts name based on
    // user selected preference. Fallback to e164.
    //
    // e164:           phone number
    // recipientName:       Signal profile username
    // showNoteToSelf: true:      show "You"
    //                 false:     show "Note to self"
    //                 undefined: show own name instead
    function getRecipientName(e164, extId, recipientName, showNoteToSelf) {
        if(!recipientName) {
            recipientName = ''
        }
        if(!e164 && !extId) {
            return recipientName
        }

        if((showNoteToSelf !== undefined) && (e164 == SetupWorker.phoneNumber)) {
            if(showNoteToSelf) {
                //: Name of the conversation with one's own number
                //% "Note to self"
                return qsTrId("whisperfish-session-note-to-self")
            } else {
                //: Name shown when replying to own messages
                //% "You"
                return qsTrId("whisperfish-sender-name-label-outgoing")
            }
        }

        var contact = null
        if (extId != null) {
            // In Sailfish OS, extId is a number
            extId = parseInt(extId)
            contact = contactsReady && extId > 0 ? resolvePeopleModel.personById(extId) : null
        }
        if (contact == null && e164 != null && e164[0] === '+') {
            // Only try to search for contact name if contact is a phone number
            contact = contactsReady ? resolvePeopleModel.personByPhoneNumber(e164, true) : null
        }
        if(SettingsBridge.prefer_device_contacts) {
            return (contact && contact.displayLabel !== '') ? contact.displayLabel : recipientName
        } else {
            return (recipientName !== '') ? recipientName : (contact ? contact.displayLabel : e164)
        }
    }

    function closeMessageNotification(sessionId, messageId) {
        console.log("Closing message notification for session", sessionId, "and message", messageId)
        if(sessionId in notificationMap) {
            for(var i in notificationMap[sessionId]) {
                // This needs to be a loose comparison for some reason
                if(notificationMap[sessionId][i].messageId == messageId) {
                    notificationMap[sessionId][i].close()
                    delete notificationMap[sessionId][i]
                    notificationMap[sessionId].splice(i, 1)

                    if(notificationMap[sessionId].length === 0) {
                        delete notificationMap[sessionId]
                    }
                    break
                }
            }
        }
        for(var i in notificationQueue) {
            if(notificationQueue[i].messageId == messageId) {
                console.log("Removing from queue")
                delete notificationQueue[i]
                notificationQueue.splice(i, 1)
                break
            }
        }
    }

    function newMissedCallNotification(data) {
        var senderName = getRecipientName(data.senderE164, undefined, data.senderName)
        var contactName = data.isGroup ? data.sessionName : senderName

        // Only ConversationPage.qml has `sessionId` property.
        if(Qt.application.state == Qt.ApplicationActive &&
           (pageStack.currentPage == _mainPage || pageStack.currentPage.sessionId == data.sessionId)) {
            if(quietMessageNotification.isSupported) {
                quietMessageNotification.publish()
            }
            return
        }

        var m = callNotification.createObject(null)
        m.itemCount = 1
        var setting = SettingsBridge.notification_privacy.toString();
        if(setting === "off") {
            return;
        } else {
            if (!data.isVideoCall) {
                //: Notification text for missed call notification
                //% "Missed voice call"
                m.body = qsTrId("whisperfish-notification-missed-voice-call")
            } else {
                //: Notification text for missed call notification
                //% "Missed video call"
                m.body = qsTrId("whisperfish-notification-missed-video-call")
            }
        }

        if(setting === "complete" || setting === "sender-only") {
            m.previewSummary = senderName
            m.summary = senderName
            if(m.subText !== undefined) {
                m.subText = contactName
            }
        }

        m.previewBody = m.body
        m.clicked.connect(function() {
            console.log("Activating session: " + data.sessionId)
            mainWindow.activate()
            showMainPage()
            pageStack.push(Qt.resolvedUrl("pages/ConversationPage.qml"), { sessionId: data.sessionId }, PageStackAction.Immediate)
        })
        // This is needed to call default action
        m.remoteActions = [ {
            "name": "default",
            "displayName": "Show Conversation",
            // Doesn't work as-is.
            // TODO: Drop in Avatar image here.
            // "icon": "harbour-whisperfish",
            "service": "org.whisperfish.session",
            "path": "/message",
            "iface": "org.whisperfish.session",
            "method": "showConversation",
            "arguments": [ "sessionId", data.sessionId ]
        } ]
        m.publish()
        m.sessionId = data.sessionId
    }

    function flushNotifications() {
        console.log("Flushing notifications, len:", notificationQueue.length)
        for(var i in notificationQueue) {
            notificationQueue[i].publish()
        }
        notificationQueue = []
    }

    function newMessageNotification(data) {
        // Only ConversationPage.qml (and its subpages) have `sessionId` property.
        if(Qt.application.state == Qt.ApplicationActive &&
           (pageStack.currentPage.sessionId == data.sessionId)) {
            console.log("Quiet notification for session " + data.sessionId)
            if(quietMessageNotification.isSupported) {
                quietMessageNotification.publish()
            }
            return
        }

        var m = messageNotification.createObject(null)
        m.itemCount = 1

        var notification_privacy = SettingsBridge.notification_privacy.toString();
        switch (notification_privacy) {
        case "complete":
            // TODO: Service messages show up as empty message (instead of even raw JSON)
            m.body = data.message
            break;
        case "minimal":
        case "sender-only":
            //: Default label for new message notification
            //% "New Message"
            m.body = qsTrId("whisperfish-notification-default-message")
            break;
        case "off":
            return;
        default:
            console.error("Unrecognised notification privacy setting:", notification_privacy);
            return;
        }

        // Does this notification replace an existing one?
        if (SettingsBridge.minimise_notify && (data.sessionId in notificationMap)) {
            var first_message = notificationMap[data.sessionId][0]
            m.replacesId = first_message.replacesId
            m.itemCount = first_message.itemCount + 1
        }

        var name = getRecipientName(data.senderE164, undefined, data.senderName)
        var contactName = data.isGroup ? data.sessionName : name

        if(notification_privacy === "complete" || notification_privacy === "sender-only") {
            m.previewSummary = data.senderName
            m.summary = data.senderName
            if(m.subText !== undefined) {
                m.subText = contactName
            }
        }
        // XXX: maybe we do want a summary?

        m.previewBody = m.body
        m.clicked.connect(function() {
            console.log("Activating session: " + data.sessionId)
            mainWindow.activate()
            showMainPage()
            pageStack.push(Qt.resolvedUrl("pages/ConversationPage.qml"), { sessionId: data.sessionId }, PageStackAction.Immediate)
        })
        // This is needed to call default action
        m.remoteActions = [ {
            "name": "default",
            "displayName": "Show Conversation",
            // Doesn't work as-is.
            // TODO: Drop in Avatar image here.
            // "icon": "harbour-whisperfish",
            "service": "org.whisperfish.session",
            "path": "/message",
            "iface": "org.whisperfish.session",
            "method": "showConversation",
            "arguments": [ "sessionId", data.sessionId ]
        } ]
        if (!ClientWorker.queueEmpty) {
            console.log("Adding to queue")
            notificationQueue.push(m)
        } else {
            console.log("Publishing immediately")
            m.publish()
        }
        m.messageId = data.messageId
        if(data.sessionId in notificationMap && !SettingsBridge.minimise_notify) {
              notificationMap[data.sessionId].push(m)
        } else {
              notificationMap[data.sessionId] = [m]
        }
    }

    // Qt 5.6 QML version of JavaScript doesn't have String.prototype.replaceAll(),
    // so we have to implement it ourselves. ECMAScript 2021 has it.
    // https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/replaceAll
    // https://stackoverflow.com/questions/1144783/how-do-i-replace-all-occurrences-of-a-string-in-javascript
    function escapeRegExp(string) {
        return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    }

    function replaceAll(str, find, replace) {
        return str.replace(new RegExp(escapeRegExp(find), 'g'), replace)
    }

    Connections {
        target: ClientWorker
        onMessageReceived: { }
        onMessageReactionReceived: { }
        onMessageReceipt: {
            if(sid > 0 && mid > 0) {
                closeMessageNotification(sid, mid)
            }
        }
        onNotifyMessage: {
            if (data.message !== undefined) {
                newMessageNotification(data)
            } else if (data.isVideoCall !== undefined) {
                newMissedCallNotification(data)
            } else {
                console.error("Unhandled message notification request:", JSON.stringify(data))
            }
        }
        onQueueEmptyChanged: {
            if (ClientWorker.queueEmpty) {
                flushNotifications()
            }
        }
        onMessageNotSent: { }
        onProofRequested: {
            if(proofCaptchaToken === '') {
                proofCaptchaToken = token
                console.log("Captcha requested with token length", token.length)
                pageStack.push(Qt.resolvedUrl("pages/ProofSubmitPage.qml"), { captchaToken: token })
            } else {
                console.log("Ignoring repeated captcha requests")
            }
        }
        onMessageSent: { }
        onPromptResetPeerIdentity: {
            if (fatalOccurred) return
            pageStack.push(Qt.resolvedUrl("pages/PeerIdentityChanged.qml"), { source: source })
        }
        onCloseNotification: {
            if (fatalOccurred) return
            closeMessageNotification(sid, mid)
        }
    }

    Connections {
        target: SetupWorker
        onClientFailed: {
            console.log("[FATAL] client failed")
            //: Failed to setup signal client error message
            //% "Failed to setup Signal client"
            showFatalError(qsTrId("whisperfish-fatal-error-setup-client"))
        }
        onInvalidDatastore: {
            //: Failed to setup datastore error message
            //% "Failed to setup data storage"
            showFatalError(qsTrId("whisperfish-fatal-error-invalid-datastore"))
        }
    }

    Connections {
        // Calls is a global, injected by the client actor.
        target: calls

        onRingingChanged: {
            if (calls.ringingRecipientId != -1 && SettingsBridge.debug_mode) {
                pageStack.push(
                    Qt.resolvedUrl("pages/RingingDialog.qml"), { }
                )
            }
        }

        onHungup: {
            console.log("Hung up notification")
        }
    }

    Connections {
        target: Qt.application
        onStateChanged: {
            if(Qt.application.state == Qt.ApplicationActive) {
                AppState.setActive()
            }
        }
    }

    Connections {
        target: AppState
        onActivate: mainWindow.activate()
    }

    Connections {
        target: RootApp
        onLastWindowClosed: {
            showMainPage()
            AppState.setClosed()
            if (AppState.mayExit()) {
                Qt.quit();
            }
        }
    }

    DBusInterface {
        id: dbusSpeechInterface

        // https://github.com/mkiol/dsnote/blob/main/dbus/org.mkiol.Speech.xml
        service: 'org.mkiol.Speech'
        path: '/'
        iface: 'org.mkiol.Speech'

        // XXX: these are undocumented
        watchServiceStatus: true
        signalsEnabled: true
        propertiesEnabled: true

        // 3 == Idle
        property bool available: installed && _state === 3 && _autoSTTAvailable
        property bool installed: status == DBusInterface.Available

        property var _state
        property bool _autoSTTAvailable: false

        Component.onCompleted: {
            // We need to read e.g. State once to trigger updates
            _state = getProperty("State");
        }

        function statePropertyChanged(state) {
            console.log("statePropertyChanged:", state)
            _state = state
        }

        function sttLangsPropertyChanged(langs) {
            console.log("sttLangsPropertyChanged:", JSON.stringify(langs))
            _autoSTTAvailable = "auto" in langs;
            console.log("Automatic language detection available:", _autoSTTAvailable);
        }
    }

    DBusAdaptor {
        service: "be.rubdos.whisperfish"
        path: "/be/rubdos/whisperfish/app"
        iface: "be.rubdos.whisperfish.app"

        function show() {
            console.log("DBus app.show() call received")
            if(Qt.application.state == Qt.ApplicationActive) {
                return
            }

            mainWindow.activate()
            if (AppState.isClosed()) {
                showMainPage()
            }
        }

        function quit() {
            console.log("DBus app.quit() call received")
            Qt.quit()
        }

        function handleShareV1(clientId, source, content) {
            console.log("DBus app.handleShare() (v1) call received");
            console.log("DBus Share Client:", clientId);
            console.log("DBus source:", source);
            console.log("DBus content:", content)
            pageStack.push(
                Qt.resolvedUrl("pages/ShareDestinationV1.qml"),
                {
                    source: source,
                    content: content
                }
            )
            mainWindow.activate()
            dbusShareClient.call("done")
        }

        function handleShareV2(clientId, shareObject) {
            console.log("DBus app.handleShare() (v2) call received");
            console.log("DBus Share Client:", clientId);
            console.log("DBus Share object:", JSON.stringify(shareObject));

            shareClientId = clientId
            pageStack.push(
                Qt.resolvedUrl("pages/ShareDestinationV2.qml"),
                { shareObject: shareObject },
                PageStackAction.Immediate
            )
            mainWindow.activate()
            dbusShareClient.call("done")
        }
    }
    DBusInterface {
        id: dbusShareClient
        service: "be.rubdos.whisperfish.shareClient.c" + shareClientId
        path: "/be/rubdos/whisperfish/shareClient/c" + shareClientId
        iface: "be.rubdos.whisperfish.shareClient"
    }

    function clearNotifications(sessionId) {
        // Close out any existing notifications for the session
        if(sessionId in notificationMap) {
            for(var i in notificationMap[sessionId]) {
                notificationMap[sessionId][i].close()
                delete notificationMap[sessionId][i]
            }
            delete notificationMap[sessionId]
        }
    }

    function showFatalError(message) {
        fatalOccurred = true
        // We don't clear the stack to keep transition animations
        // clean. FatalErrorPage will block any further navigation.
        pageStack.push(Qt.resolvedUrl("pages/FatalErrorPage.qml"), {
                           errorMessage: message
                       })
    }

    function showMainPage(operationType) {
        if (fatalOccurred) return

        if(operationType === undefined) {
            operationType = PageStackAction.Immediate
        }

        if (_mainPage) {
            pageStack.pop(_mainPage, operationType)
        } else {
            pageStack.replaceAbove(null, Qt.resolvedUrl("pages/MainPage.qml"), {}, operationType)
            _mainPage = pageStack.currentPage
        }
    }

    function newMessage(operationType) {
        if (fatalOccurred) return
        showMainPage()
        pageStack.push(Qt.resolvedUrl("pages/NewMessage.qml"), { }, operationType)
    }

    function __translation_stub() {
        // QML-lupdate mirror for harbour-whisperfish.profile

        //: Permission for Whisperfish data storage
        //% "Whisperfish data storage"
        var f = qsTrId("permission-la-data");

        //: Permission description for Whisperfish data storage
        //% "Store configuration and messages"
        var f = qsTrId("permission-la-data_description");
    }
}
