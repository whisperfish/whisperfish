import QtQuick 2.2
import Sailfish.Contacts 1.0

ContactSelectPage {

    property QtObject recipient

    //: Link Signal user to Sailfish OS contact page title
    //% "Select contact"
    title: qsTrId("whisperfish-link-contact-page-title")
    searchActive: true
    onContactClicked: {
        console.debug("Linking recipient", recipient.recipientId, "with Sailfish OS contact", contact.id)
        ClientWorker.linkRecipient(recipient.recipientId, contact.id.toString())
        pageStack.pop()
    }
}

