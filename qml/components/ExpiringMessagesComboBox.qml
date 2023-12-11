import QtQuick 2.2
import Sailfish.Silica 1.0

Column {
    id: root
    property int duration: -1 // Expiring messages timeout, in seconds.
    property int newDuration
    property bool blockUpdates: false

    function updateNewDuration() {
        switch (unitComboBox.unit) {
            case "s": newDuration = timeComboBox.amount;          break;
            case "m": newDuration = timeComboBox.amount * 60;     break;
            case "h": newDuration = timeComboBox.amount * 3600;   break;
            case "d": newDuration = timeComboBox.amount * 86400;  break;
            case "w": newDuration = timeComboBox.amount * 604800; break;
            default: console.log("Unknown time unit:", unitComboBox.unit)
        }
    }

    function getModelCount(unit) {
        switch (unit) {
            case "s": return 59;
            case "m": return 59;
            case "h": return 23;
            case "d": return  6;
            case "w": return  4;
            default:  return 59;
        }
    }

    function getExpiryIndex(seconds) {
        switch (seconds) {
            case -1:      return 0; // Off
            case 30:      return 1; // 30s
            case 300:     return 2; // 5min
            case 3600:    return 3; // 1h
            case 28800:   return 4; // 8h
            case 86400:   return 5; // 1d
            case 604800:  return 6; // 1w
            case 2419200: return 7; // 4w
            default:      return 8; // custom
        }
    }

    function setUnitIndex() {

        if      (newDuration >= 604800) { unitComboBox.currentIndex = 4 } // weeks
        else if (newDuration >=  86400) { unitComboBox.currentIndex = 3 } // days
        else if (newDuration >=   3600) { unitComboBox.currentIndex = 2 } // hours
        else if (newDuration >=     60) { unitComboBox.currentIndex = 1 } // minutes
        else                            { unitComboBox.currentIndex = 0 } // seconds
        unitComboBox.unit = ["s","m","h","d","w"][unitComboBox.currentIndex]
    }


    function setTimeIndex() {
        switch (unitComboBox.unit) {
            case "w": timeComboBox.currentIndex = Math.min( 4, Math.round(newDuration / 604800)) - 1; break;
            case "d": timeComboBox.currentIndex = Math.min( 6, Math.round(newDuration /  86400)) - 1; break;
            case "h": timeComboBox.currentIndex = Math.min(23, Math.round(newDuration /   3600)) - 1; break;
            case "m": timeComboBox.currentIndex = Math.min(59, Math.round(newDuration /     60)) - 1; break;
            case "s": timeComboBox.currentIndex = Math.min(59, Math.round(newDuration         )) - 1; break;
            default: console.log("Unknown time unit:", unitComboBox.unit)
        }
    }

    ComboBox {
        id: expiryComboBox
        //: Group/conversation info page, expiring messages setting
        //% "Disappearing messages"
        label: qsTrId("whisperfish-disappearing-messages-setting")
        //: Group/conversation info page, expiring messages description
        //% "Set or disable message destruction after a certain time after reading. Only affects messages sent after changing this option."
        description: qsTrId("whisperfish-disappearing-messages-description")
        Component.onCompleted: {
            currentIndex = getExpiryIndex(duration)
            if (currentIndex == 8 && duration > 0) {
                newDuration = duration
            }
            handleClick()
        }
        property var durations: [-1, 30, 300, 3600, 28800, 86400, 604800, 2419200, 0]
        property var texts: [
            //: Disappearing messages: off
            //% "Off"
            qsTrId("whisperfish-disappearing-messages-off"),

            //: Disappearing messages duration in seconds
            //% "%n second(s)"
            qsTrId("whisperfish-disappearing-messages-seconds", 30),

            //: Disappearing messages duration in minutes
            //% "%n minute(s)"
            qsTrId("whisperfish-disappearing-messages-minutes", 5),

            //: Disappearing messages duration in hours
            //% "%n hour(s)"
            qsTrId("whisperfish-disappearing-messages-hours", 1),

            // See above
            qsTrId("whisperfish-disappearing-messages-hours", 8),

            //: Disappearing messages duration in days
            //% "%n day(s)"
            qsTrId("whisperfish-disappearing-messages-days", 1),

            //: Disappearing messages duration in weeks
            //% "%n week(s)"
            qsTrId("whisperfish-disappearing-messages-weeks", 1),

            // See above
            qsTrId("whisperfish-disappearing-messages-weeks", 4),

            //: Disappearing messages: custom duration
            //% "Other"
            qsTrId("whisperfish-disappearing-messages-custom")
        ]
        menu: ContextMenu {
            Repeater {
                model: 9
                MenuItem {
                    text: expiryComboBox.texts[index]
                    property int value: expiryComboBox.durations[index]
                    onClicked: expiryComboBox.handleClick()
                }
            }
        }

        function handleClick() {
            if (currentIndex == 0) {
                // Off = -1
                newDuration = -1
            }
            else if (currentIndex > 0 && currentIndex < 8) {
                // Ensure valid index calculation later
                newDuration = Math.max(1, currentItem.value)
            }
            setUnitIndex()
            setTimeIndex()
        }
    }

    ComboBox {
        id: timeComboBox
        // No animation needed, because expiryComboBox opens a sub-page
        visible: expiryComboBox.currentIndex == 8
        //: Disappearing messages, custom "time amount" label
        //% "Amount"
        label: qsTrId("whisperfish-disappearing-messages-amount")
        property int amount: currentIndex + 1
        menu: ContextMenu {
            Repeater {
                model: getModelCount(unitComboBox.unit)
                MenuItem {
                    text: index + 1
                    onClicked: updateNewDuration()
                }
            }
        }
    }

    ComboBox {
        id: unitComboBox
        // No animation needed, because expiryComboBox opens a sub-page
        visible: expiryComboBox.currentIndex == 8
        //: Disappearing messages, custom "time length" label
        //% "Time unit"
        label: qsTrId("whisperfish-disappearing-messages-time-units")
        property string unit
        property var unitTexts: [
            //: Time unit: seconds
            //% "seconds"
            qsTrId("whisperfish-units-seconds", timeComboBox.amount),
            //: Time unit: minutes
            //% "minutes"
            qsTrId("whisperfish-units-minutes", timeComboBox.amount),
            //: Time unit: hours
            //% "hours"
            qsTrId("whisperfish-units-hours", timeComboBox.amount),
            //: Time unit: days
            //% "days"
            qsTrId("whisperfish-units-days", timeComboBox.amount),
            //: Time unit: weeks
            //% "weeks"
            qsTrId("whisperfish-units-weeks", timeComboBox.amount)
        ]
        menu: ContextMenu {
            Repeater {
                model: ["s","m","h","d","w"]
                MenuItem {
                    text: unitComboBox.unitTexts[index]
                    onClicked: {
                        unitComboBox.unit = modelData
                        setTimeIndex()
                        updateNewDuration()
                    }
                }
            }
        }
    }
}