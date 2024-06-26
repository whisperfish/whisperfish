/*
 * This was adapted from jolla-messages for use with Whisperfish
 *
 * Copyright (C) 2012-2015 Jolla Ltd.
 *
 * The code in this file is distributed under multiple licenses, and as such,
 * may be used under any one of the following licenses:
 *
 *   - GNU General Public License as published by the Free Software Foundation;
 *     either version 2 of the License (see LICENSE.GPLv2 in the root directory
 *     for full terms), or (at your option) any later version.
 *   - GNU Lesser General Public License as published by the Free Software
 *     Foundation; either version 2.1 of the License (see LICENSE.LGPLv21 in the
 *     root directory for full terms), or (at your option) any later version.
 *   - Alternatively, if you have a commercial license agreement with Jolla Ltd,
 *     you may use the code under the terms of that license instead.
 *
 * You can visit <https://jolla.com/legal/> for more information
 */

import QtQuick 2.2
import Sailfish.Silica 1.0
import Sailfish.Messages 1.0
import Sailfish.Contacts 1.0
import Sailfish.Telephony 1.0
import org.nemomobile.contacts 1.0
import org.nemomobile.commhistory 1.0
import "../components"

Page {
    id: newMessagePage
    objectName: "newMessagePage"

    property bool recipientSelected: recipientField.selectedContacts.count == 1
    property QtObject selectedContact: recipientSelected ? recipientField.selectedContacts.get(0) : null
    property bool isValid: recipientSelected && recipientNumber != ""
    property bool localAllowed: String(SettingsBridge.country_code) !== ""

    property string recipientNumberRaw: recipientSelected && selectedContact.propertyType === "phoneNumber" ? selectedContact.property.number : ""
    property string recipientNumber: ContactModel.format(recipientNumberRaw)
    property string recipientName: recipientSelected ? selectedContact.formattedNameText : ""
    property var numberFormat:        /^\+?[- 0-9]{4,}$/
    property var numberFormatNoLocal: /^\+[- 0-9]{4,}$/

    _clickablePageIndicators: !(isLandscape && recipientField.activeFocus)

    function showError(message) {
        errorLabel.hidden = true
        errorLabel.text = message
        errorLabel.hidden = false
    }

    onRecipientNumberRawChanged: {
        if (recipientNumberRaw === "") {
            showError('') // reset error
            return
        }
        if (!numberFormat.test(recipientNumberRaw)) {
            //: invalid recipient phone number: invalid characters
            //% "This phone number contains invalid characters."
            showError(qsTrId("whisperfish-recipient-number-invalid-chars"))
        } else if (!localAllowed && !numberFormatNoLocal.test(recipientNumberRaw)) {
            //: invalid recipient phone number: local numbers are not allowed
            //% "Please set a country code in the settings, "
            //% "or use the international format."
            showError(qsTrId("whisperfish-recipient-local-number-not-allowed"))
        } else if (String(ContactModel.format(recipientNumberRaw)) === "") {
            //: invalid recipient phone number: failed to format
            //% "This phone number appears to be invalid."
            showError(qsTrId("whisperfish-recipient-number-invalid-unspecified"))
        } else {
            showError('')  // reset error
        }
    }

    SilicaFlickable {
        id: newMessage
        focus: true
        contentHeight: content.y + content.height
        anchors.fill: parent

        RemorsePopup { id: remorse }

        Column {
            id: content
            y: newMessagePage.isLandscape ? Theme.paddingMedium : 0
            width: newMessage.width
            Item {
                width: newMessage.width
                height: Math.max(recipientField.height, newMessage.height - textInput.height - content.y)

                Column {
                    id: recipientHeader
                    width: parent.width
                    PageHeader {
                        //: New message page title
                        //% "New message"
                        title: qsTrId("whisperfish-new-message-title")
                        visible: newMessagePage.isPortrait
                    }

                    RecipientField {
                        id: recipientField

                        actionType: Telephony.Message
                        width: parent.width
                        recentContactsCategoryMask: CommHistory.VoicecallCategory | CommHistory.VoicemailCategory
                        contactSearchModel: PeopleModel { filterType: PeopleModel.FilterNone }
                        requiredProperty: PeopleModel.PhoneNumberRequired
                        showLabel: newMessagePage.isPortrait

                        multipleAllowed: false

                        //: New message recipient label
                        //% "Recipient"
                        placeholderText: qsTrId("whisperfish-new-message-recipient")

                        //: Summary of all selected recipients, e.g. "Bob, Jane, 75553243"
                        //% "Recipient"
                        summaryPlaceholderText: qsTrId("whisperfish-new-message-recipient")

                        onSelectionChanged: {
                            console.log("Selected recipient name: " + recipientName);
                            console.log("Selected recipient number: " + recipientNumber);
                        }
                        onHasFocusChanged: {
                            if (!hasFocus) textInput.forceActiveFocus()
                            errorLabel.hidden = hasFocus // hide errors while selecting
                        }
                    }
                }
                Label {
                    id: errorLabel
                    property bool hidden: false
                    opacity: hidden ? 0.0 : 1.0
                    width: parent.width - 4*Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    textFormat: Text.AutoText
                    horizontalAlignment: Qt.AlignHCenter
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.highlightColor
                    Behavior on opacity { FadeAnimator { } }
                    anchors {
                        bottom: parent.bottom
                        horizontalCenter: parent.horizontalCenter
                    }
                }
            }

            ChatTextInput {
                id: textInput
                width: parent.width
                enablePersonalizedPlaceholder: true
                placeholderContactName: _contact !== null ? _contact.displayLabel : ''
                showSeparator: false
                enableSending: recipientNumber.length > 0
                clearAfterSend: recipientNumber.length > 0
                property var _contact: mainWindow.contactsReady ? resolvePeopleModel.personByPhoneNumber(recipientNumber, true) : null

                onSendMessage: {
                    // TODO This should be handled completely in the backend.
                    // TODO errors should be handled (asynchronously)
                    if (recipientNumber.length > 0) {
                        var firstAttachedPath = (attachments.length > 0 ? attachments[0].data : '')
                        MessageModel.createMessage(recipientNumber, text, '', firstAttachedPath, false, isVoiceNote)

                        for (var i = 1; i < attachments.length; i++) {
                            MessageModel.createMessage(recipientNumber, '', '', attachments[i].data, true, isVoiceNote)
                        }

                        pageStack.pop() // TODO open the new chat instead of returning to the main page
                    } else {
                        //: Invalid recipient error
                        //% "Invalid recipient"
                        showError(qsTrId("whisperfish-error-invalid-recipient"))
                    }
                }
            }
        }
        VerticalScrollDecorator {}
    }
}
