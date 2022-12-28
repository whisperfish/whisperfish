import QtQuick 2.2
import Sailfish.Silica 1.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0
import org.nemomobile.contacts 1.0
import "pages"

// Note: This is the main QML file for Whisperfish.
// The reCaptcha helper uses harbour-whisperfish.qml by design.

ApplicationWindow
{
    id: mainWindow
    cover: Qt.resolvedUrl("cover/CoverPage.qml")
    initialPage: Component { LandingPage { } }
    allowedOrientations: Orientation.All
    _defaultPageOrientations: Orientation.All
    _defaultLabelFormat: Text.PlainText

    readonly property string mainPageName: "mainPage"
    readonly property string conversationPageName: "conversationPage"
    property var notificationMap: ({})

    // setting this to "true" will block global navigation
    // methods (showMainPage() etc.)
    property bool fatalOccurred: false

    property alias contactsReady: resolvePeopleModel.populated

    property string shareClientId: ""

    PeopleModel {
        id: resolvePeopleModel

        // Specify the PhoneNumberRequired flag to ensure that all phone number
        // data will be loaded before the model emits populated.
        // This ensures that we resolve numbers to contacts appropriately, in
        // the case where we attempt to message a newly-created contact via
        // the action shortcut icon in the contact card.
        requiredProperty: PeopleModel.PhoneNumberRequired
    }

    Component {
        id: messageNotification
        Notification {
            property int mid
            appIcon: "harbour-whisperfish"
            appName: "Whisperfish"
            category: "harbour-whisperfish-message"
        }
    }

    Notification {
        id: genericNotification
        appIcon: "harbour-whisperfish"
        appName: "Whisperfish"
        category: "harbour-whisperfish-message"
        previewBody: genericNotification.body
        previewSummary: genericNotification.summary
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

    function getGroupAvatar(groupId) {
        if(!groupId || groupId === '') {
            return ''
        }

        var group_avatar = "file://" + SettingsBridge.avatar_dir + "/" + groupId
        var group_avatar_ok = SettingsBridge.avatarExists(groupId)

        return group_avatar_ok ? group_avatar : ''
    }

    // Return peer contacts avatar or Signal profile avatar based on
    // user selected preference. Do not use for groups (there's no choice).
    function getRecipientAvatar(e164, uuid) {
        // Only try to search for contact name if contact is a phone number
        var contact = (contactsReady && e164[0] === '+') ? resolvePeopleModel.personByPhoneNumber(e164, true) : null

        var contact_avatar = (contact && contact.avatarPath) ? contact.avatarPath.toString() : null
        var contact_avatar_ok = (contact_avatar !== null) && (contact_avatar !== 'image://theme/icon-m-telephony-contact-avatar')

        var signal_avatar = "file://" + SettingsBridge.avatar_dir + "/" + uuid
        var signal_avatar_ok = SettingsBridge.avatarExists(uuid)

        if(!contact_avatar_ok && !signal_avatar_ok) {
            return ''
        }

        if(SettingsBridge.prefer_device_contacts) {
            return contact_avatar_ok ? contact_avatar : signal_avatar
        } else {
            return signal_avatar_ok ? signal_avatar : contact_avatar
        }
    }

    // Return either given peer name or device contacts name based on
    // user selected preference. Fallback to e164.
    //
    // e164:           phone number
    // peerName:       Signal profile username
    // showNoteToSelf: true:      show "You"
    //                 false:     show "Note to self"
    //                 undefined: show own name instead
    function getRecipientName(e164, peerName, shownNoteToSelf) {
        if(!peerName) {
            peerName = ''
        }
        if(!e164) {
            return peerName
        }
        if((shownNoteToSelf !== undefined) && (e164 === SetupWorker.phoneNumber)) {
            if(shownNoteToSelf) {
                //: Name of the conversation with one's own number
                //% "Note to self"
                return qsTrId("whisperfish-session-note-to-self")
            } else {
                //: Name shown when replying to own messages
                //% "You"
                return qsTrId("whisperfish-sender-name-label-outgoing")
            }
        }

        // Only try to search for contact name if contact is a phone number
        var contact = (contactsReady && e164[0] === '+') ? resolvePeopleModel.personByPhoneNumber(e164, true) : null
        if(SettingsBridge.prefer_device_contacts) {
            return (contact && contact.displayLabel !== '') ? contact.displayLabel : peerName
        } else {
            return (peerName !== '') ? peerName : (contact ? contact.displayLabel : e164)
        }
    }

    function activateSession(sid) {
        MessageModel.load(sid)
    }

    function closeMessageNotification(sid, mid) {
        if(sid in notificationMap) {
            for(var i in notificationMap[sid]) {

                if(notificationMap[sid][i].mid === mid) {
                    notificationMap[sid][i].close()
                    delete notificationMap[sid][i]
                    notificationMap[sid].splice(i, 1)

                    if(notificationMap[sid].length === 0) {
                        delete notificationMap[sid]
                    }
                    break
                }
            }
        }
    }

    function newMessageNotification(sid, mid, sessionName, senderName, senderIdentifier, senderUuid, message, isGroup) {
        var name = getRecipientName(senderIdentifier, senderName)
        var contactName = isGroup ? sessionName : name

        var avatar = getRecipientAvatar(senderIdentifier, senderUuid)

        if(Qt.application.state == Qt.ApplicationActive &&
           (pageStack.currentPage.objectName == mainPageName ||
           (sid == MessageModel.sessionId && pageStack.currentPage.objectName == conversationPageName))) {
            if(quietMessageNotification.isSupported) {
                quietMessageNotification.publish()
            }
            return
        }

        var m
        if(SettingsBridge.show_notify_message) {
            m = messageNotification.createObject(null)
            m.body = message
            m.itemCount = 1
        } else {
            if (sid in notificationMap) {
                m = notificationMap[sid][0]
                m.itemCount++
            } else {
                m = messageNotification.createObject(null)
                m.itemCount = 1
            }
            //: Default label for new message notification
            //% "New Message"
            m.body = qsTrId("whisperfish-notification-default-message")
        }

        if (SettingsBridge.minimise_notify && (sid in notificationMap)) {
            var first_message = notificationMap[sid][0]
            m.replacesId = first_message.replacesId
            m.itemCount = first_message.itemCount + 1
        }

        m.previewSummary = name
        m.previewBody = m.body
        m.summary = name
        if(m.subText !== undefined) {
            m.subText = contactName
        }
        m.clicked.connect(function() {
            console.log("Activating session: "+sid)
            mainWindow.activate()
            showMainPage()
            mainWindow.activateSession(sid)
            pageStack.push(Qt.resolvedUrl("pages/ConversationPage.qml"), { peerName: contactName, profilePicture: avatar }, PageStackAction.Immediate)
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
            "arguments": [ "sid", sid ]
        } ]
        m.publish()
        m.mid = mid
        if(sid in notificationMap) {
              notificationMap[sid].push(m)
        } else {
              notificationMap[sid] = [m]
        }
    }

    Connections {
        target: ClientWorker
        onMessageReceived: {
            if(sid == MessageModel.sessionId && pageStack.currentPage.objectName == conversationPageName) {
                SessionModel.add(sid, true)
                MessageModel.add(mid)
            } else {
                SessionModel.add(sid, false)
            }
        }
        onMessageReactionReceived: {
            if(sid == MessageModel.sessionId && pageStack.currentPage.objectName == conversationPageName) {
                MessageModel.reload_message(mid)
            }
        }
        onMessageReceipt: {
            if(mid > 0 && sid === MessageModel.sessionId && pageStack.currentPage.objectName == conversationPageName) {
                MessageModel.markReceived(mid)
            }

            if(sid > 0) {
                SessionModel.markReceived(sid)
            }

            if(sid > 0 && mid > 0) {
                closeMessageNotification(sid, mid)
            }
        }
        onNotifyMessage: {
            newMessageNotification(sid, mid, sessionName, senderName, senderIdentifier, senderUuid, message, isGroup)
        }
        onMessageNotSent: {
            if(sid == MessageModel.sessionId && pageStack.currentPage.objectName == conversationPageName) {
                MessageModel.markFailed(mid)
            }
        }
        onProofRequested: {
            console.log("Proof of type", type, "with token", token, "requested")
            pageStack.push(Qt.resolvedUrl("ProofSubmitPage.qml"), { recaptchaToken: token })
        }
        onMessageSent: {
            if(sid == MessageModel.sessionId && pageStack.currentPage.objectName == conversationPageName) {
                SessionModel.markSent(sid, message)
                MessageModel.markSent(mid)
            } else {
                SessionModel.markSent(sid, message)
            }
        }
        onPromptResetPeerIdentity: {
            if (fatalOccurred) return
            pageStack.push(Qt.resolvedUrl("pages/PeerIdentityChanged.qml"), { source: source })
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
            AppState.setClosed()
            if (AppState.mayExit()) {
                Qt.quit();
            }
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

    function clearNotifications(sid) {
        // Close out any existing notifications for the session
        if(sid in notificationMap) {
            for(var i in notificationMap[sid]) {
                notificationMap[sid][i].close()
            }
            delete notificationMap[sid]
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
        pageStack.replaceAbove(null, Qt.resolvedUrl("pages/MainPage.qml"), {},
                               operationType !== undefined ? operationType :
                                                             PageStackAction.Immediate)
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
