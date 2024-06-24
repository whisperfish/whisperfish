import QtQuick 2.2
import Sailfish.Silica 1.0
import Sailfish.Messages 1.0
import Sailfish.Contacts 1.0
import Sailfish.Telephony 1.0
import org.nemomobile.contacts 1.0
import org.nemomobile.commhistory 1.0
import "../components"

Page {
    id: newGroupPage
    objectName: "newGroupPage"

    property Label errorLabel

    property var selectedContacts: recipientField.selectedContacts

    _clickablePageIndicators: !(isLandscape && recipientField.activeFocus)

    SilicaFlickable {
        id: newGroup
        focus: true
        contentHeight: content.y + content.height
        anchors.fill: parent

        Column {
            id: content
            y: newGroupPage.isLandscape ? Theme.paddingMedium : 0
            width: newGroup.width
            Item {
                width: newGroup.width
                height: Math.max(recipientField.height+groupName.height, newGroup.height - textInput.height - content.y)

                Column {
                    id: recipientHeader
                    width: parent.width
                    PageHeader {
                        //: New group page title
                        //% "New Group"
                        title: qsTrId("whisperfish-new-group-title")
                        visible: newGroupPage.isPortrait
                    }

                    TextField {
                        id: groupName
                        width: parent.width
                        //: Group name label
                        //% "Group Name"
                        label: qsTrId("whisperfish-group-name-label")
                        //: Group name placeholder
                        //% "Group Name"
                        placeholderText: qsTrId("whisperfish-group-name-placeholder")
                        placeholderColor: Theme.highlightColor
                        horizontalAlignment: TextInput.AlignLeft
                     }

                    RecipientField {
                        id: recipientField

                        actionType: Telephony.Message
                        width: parent.width
                        recentContactsCategoryMask: CommHistory.VoicecallCategory | CommHistory.VoicemailCategory
                        contactSearchModel: PeopleModel { filterType: PeopleModel.FilterNone }
                        requiredProperty: PeopleModel.PhoneNumberRequired
                        showLabel: newGroupPage.isPortrait

                        multipleAllowed: true

                        //: New group message members label
                        //% "Members"
                        placeholderText: qsTrId("whisperfish-new-group-message-members")

                        //: Summary of all selected recipients, e.g. "Bob, Jane, 75553243"
                        //% "Members"
                        summaryPlaceholderText: qsTrId("whisperfish-new-group-message-members")

                        onSelectionChanged: {
                            console.log("Selected contact count: " + selectedContacts.count);
                        }
                        onHasFocusChanged: if (!hasFocus) textInput.forceActiveFocus()
                    }
                }
                ErrorLabel {
                    id: errorLabel
                    visible: text.length > 0
                    anchors {
                        bottom: parent.bottom
                        bottomMargin: -Theme.paddingSmall
                    }
                }
            }

            ChatTextInput {
                id: textInput
                width: parent.width
                enablePersonalizedPlaceholder: false
                showSeparator: false
                enableSending: selectedContacts.count > 0 && groupName.text != ""
                clearAfterSend: selectedContacts.count > 0 && groupName.text != ""
                enableAttachments: false // TODO support attachments

                onSendMessage: {
                    // TODO rewrite
                    // TODO support attachments
                    if (selectedContacts.count === 0) {
                        //: Invalid recipient error
                        //% "Please select group members"
                        errorLabel.text = qsTrId("whisperfish-error-invalid-group-members")
                    } else if(groupName.text == "") {
                        //: Invalid group name error
                        //% "Please name the group"
                        errorLabel.text = qsTrId("whisperfish-error-invalid-group-name")
                    } else {
                        var numbers = []
                        for (var i = 0; i < selectedContacts.count; ++i) {
                            var contact = selectedContacts.get(i);
                            var phone = ContactModel.format(contact.property.number);
                            if (phone === "") {
                                console.log("Skipping invalid number" + contact.formattedNameText + " (" + contact.property.number + ")");
                                continue;
                            }
                            numbers.push(phone)
                        }
                        var source = numbers.join(",")
                        console.log("Creating group for " + source);
                        MessageModel.createMessage(source, text, groupName.text, "", false, isVoiceNote)
                        pageStack.pop()
                    }
                }
            }
        }
        VerticalScrollDecorator {}
    }
}
