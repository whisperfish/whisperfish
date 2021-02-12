import QtQuick 2.2
import Sailfish.Silica 1.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0
import org.nemomobile.contacts 1.0
import "pages"

ApplicationWindow
{
    id: mainWindow
    cover: Qt.resolvedUrl("cover/CoverPage.qml")
    initialPage: Component { LandingPage { } }
    allowedOrientations: Orientation.All
    _defaultPageOrientations: Orientation.All
    _defaultLabelFormat: Text.PlainText

    readonly property string mainPageName: "mainPage"
    property var notificationMap: ({})

    // setting this to "true" will block global navigation
    // methods (showMainPage() etc.)
    property bool fatalOccurred: false

    property alias contactsReady: resolvePeopleModel.populated

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
        Notification {}
    }

    function activateSession(sid, name, source) {
        console.log("Activating session for source: "+source)
        SessionModel.markRead(sid)
        MessageModel.load(sid, name)
    }

    function newMessageNotification(sid, source, message, isGroup) {
        var contact = resolvePeopleModel.personByPhoneNumber(source);
        var name = contact ? contact.displayLabel : source;

        if(Qt.application.state == Qt.ApplicationActive &&
           (pageStack.currentPage.objectName == mainPageName ||
           (sid == MessageModel.sessionId && pageStack.currentPage.objectName == "conversation"))) {
           return
        }

        var m = messageNotification.createObject(null)
        if(SettingsBridge.boolValue("show_notify_message")) {
            m.body = message
        } else {
            //: Default label for new message notification
            //% "New Message"
            m.body = qsTrId("whisperfish-notification-default-message")
        }
        m.category = "harbour-whisperfish-message"
        m.previewSummary = name
        m.previewBody = m.body
        m.summary = name
        m.clicked.connect(function() {
            clearNotifications(sid)
            console.log("Activating session: "+sid)
            mainWindow.activate()
            showMainPage()
            mainWindow.activateSession(sid, name, source)
            pageStack.push(Qt.resolvedUrl("pages/Conversation.qml"), {}, PageStackAction.Immediate)
        })
        // This is needed to call default action
        m.remoteActions = [ {
            "name": "default",
            "displayName": "Show Conversation",
            "icon": "harbour-whisperfish",
            "service": "org.whisperfish.session",
            "path": "/message",
            "iface": "org.whisperfish.session",
            "method": "showConversation",
            "arguments": [ "sid", sid ]
        } ]
        m.publish()
        if(sid in notificationMap) {
              notificationMap[sid].push(m)
        } else {
              notificationMap[sid] = [m]
        }
    }

    Connections {
        target: ClientWorker
        onMessageReceived: {
            if(sid == MessageModel.sessionId && pageStack.currentPage.objectName == "conversation") {
                SessionModel.add(sid, true)
                MessageModel.add(mid)
            } else {
                SessionModel.add(sid, false)
            }
        }
        onMessageReceipt: {
            if(mid > 0 && pageStack.currentPage.objectName == "conversation") {
                MessageModel.markReceived(mid)
            }

            if(sid > 0) {
                SessionModel.markReceived(sid)
            }
        }
        onNotifyMessage: {
            newMessageNotification(sid, source, message, isGroup)
        }
        onMessageSent: {
            if(sid == MessageModel.sessionId && pageStack.currentPage.objectName == "conversation") {
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

    DBusAdaptor {
        service: "be.rubdos.whisperfish.app"
        path: "/be/rubdos/whisperfish"
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
    }

    function clearNotifications(sid) {
        // Close out any existing notifications for the session
        if(sid in notificationMap) {
            for(var i = 0; i < notificationMap[sid].length; i++) {
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
