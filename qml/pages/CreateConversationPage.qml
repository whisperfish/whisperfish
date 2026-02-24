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

    function attemptTransition() {
        if (sessionId != -1) {
            if (pageStack.busy) {
                pageStack.completeAnimation();
            } else {
                pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), { sessionId: sessionId });
            }
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


    PageHeader {
        //: Page header title when a new conversation is being created
        //% "Creating conversation"
        title: createConversation.hasName ? createConversation.name : qsTrId("whisperfish-creating-conversation-title")
        description: createConversation.hasName ? qsTrId("whisperfish-creating-conversation-title") : ""
    }

    BusyIndicator {
        size: BusyIndicatorSize.Large
        anchors.centerIn: parent
        running: !createConversation.invalid && !createConversation.ready
    }
}

